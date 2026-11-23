"""Build the PG19 PL/pgSQL compiler with libpg_query's catalog-free adapters."""
import argparse
import re
import subprocess
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
LIBPG_QUERY = HERE.parent / "libpg_query"
MOCKS = LIBPG_QUERY / "scripts/mocks"
PL = Path("src/pl/plpgsql/src")
BACKEND = Path("src/backend")

# The PG19 compiler uses libpg_query’s offline replacements for catalog lookups.
ADAPTERS = {
    PL / "pl_comp.c": ["build_datatype", "plpgsql_parse_wordtype", "plpgsql_parse_wordrowtype",
                       "plpgsql_parse_cwordtype", "plpgsql_parse_cwordrowtype"],
    BACKEND / "utils/cache/syscache.c": ["SearchSysCache1", "GetSysCacheOid", "ReleaseSysCache",
                                         "SysCacheGetAttr", "SysCacheGetAttrNotNull"],
    BACKEND / "utils/cache/funccache.c": ["cfunc_resolve_polymorphic_argtypes"],
    BACKEND / "utils/fmgr/funcapi.c": ["get_func_arg_info"],
    BACKEND / "commands/functioncmds.c": ["interpret_function_parameter_list"],
    BACKEND / "catalog/namespace.c": ["recomputeNamespacePath", "LookupExplicitNamespace",
                                      "DeconstructQualifiedName", "TypeIsVisible", "get_collation_oid"],
    BACKEND / "parser/parse_type.c": ["typenameTypeMod", "LookupTypeNameExtended"],
    BACKEND / "parser/parse_expr.c": ["transformExpr"],
    BACKEND / "parser/parse_coerce.c": ["coerce_to_specific_type"],
    BACKEND / "parser/parse_collate.c": ["assign_expr_collations"],
    BACKEND / "optimizer/util/var.c": ["contain_var_clause"],
    BACKEND / "utils/adt/regproc.c": ["format_procedure"],
    BACKEND / "utils/adt/format_type.c": ["printTypmod"],
    BACKEND / "catalog/pg_proc.c": ["function_parse_error_transpose"],
}


def function(text, name):
    match = re.search(r"^" + name + r"\([^;]*?\n\{.*?^\}", text, re.M | re.S)
    if not match:
        raise ValueError(f"missing function definition for {name}")
    return match


def adapt(source, relative, work):
    original = (source / relative).read_text()
    for name in ADAPTERS[relative]:
        target = function(original, name)
        mock = MOCKS / (name + ".c")
        if mock.exists():
            template = mock.read_text()
        else:
            extracted = "src_" + str(relative.relative_to("src")).replace("/", "_")
            template = (LIBPG_QUERY / "src/postgres" / extracted).read_text()
            template = re.sub(r";\}(?=\n)", ";\n}", template)
        replacement = function(template, name).group().split("\n{", 1)[1]
        args = target.group().split("\n{", 1)[0]
        original = original[:target.start()] + args + "\n{" + replacement + original[target.end():]
    if relative.name == "syscache.c":
        includes = '\n#include "catalog/pg_type.h"\n#include "catalog/pg_collation_d.h"\n#include "pg_query_pg_type.c"\n#include "pg_query_proctup_attrs.h"\n'
        original = original.replace('#include "postgres.h"', '#include "postgres.h"' + includes, 1)
    if relative.name == "pl_comp.c":
        original = original.replace("static void plpgsql_compile_callback(", "void plpgsql_compile_callback(")
        original = original.replace("static void\nplpgsql_compile_callback(", "void\nplpgsql_compile_callback(")
    dest = work / relative.name
    dest.write_text(original)
    return dest


def build(run, work):
    source, build = run / "patched-source", run / "patched"
    if '#define PG_VERSION_NUM 190000' not in (build / "src/include/pg_config.h").read_text().splitlines():
        raise ValueError("expected the patched PG19.0 import")
    subprocess.run(["make", "-s", "-j4", "-C", build / PL, "all"], check=True)
    cat = build / "src/include/catalog"
    subprocess.run(["ruby", LIBPG_QUERY / "scripts/extract_pg_types.rb",
                    cat, work / "pg_query_pg_type.c"], check=True)
    # The public 18.6 API header also carries release constants. The probe
    # uses its declarations with the configured 19.0 backend constants.
    api = (LIBPG_QUERY / "pg_query.h").read_text()
    api = re.sub(r"^#define (?:PG_MAJORVERSION|PG_VERSION|PG_VERSION_NUM) .+$", "", api, flags=re.M)
    (work / "pg_query.h").write_text(api)
    units = [adapt(source, relative, work) for relative in ADAPTERS]
    units += [HERE / "probe.c", HERE / "routine-inputs.c", LIBPG_QUERY / "src/pg_query_parse_plpgsql.c", LIBPG_QUERY / "src/pg_query_json_plpgsql.c"]
    flags = [work, LIBPG_QUERY, LIBPG_QUERY / "src", LIBPG_QUERY / "src/include", source / PL, build / PL]
    # Preserve quoted includes of sibling/generator headers in copied source files.
    flags += sorted({root / f.parent for root in [source, build] for f in ADAPTERS})
    makefile = ["include Makefile", ".PHONY: plpgsql-reference",
                "plpgsql_includes = " + " ".join(f'-I"{d}"' for d in flags)]
    objects = []
    for unit in units:
        obj = work / (unit.stem + ".o")
        objects.append(str(obj))
        makefile += [f'{obj}: {unit}', '\t$(COMPILE.c) $(plpgsql_includes) -include "' + str(HERE / "callback.h") + '" $< -o $@']
    objects += [str(f) for f in sorted((build / PL).glob("pl_*.o")) if f.name != "pl_comp.o"]
    exclude = " ".join(str(f.relative_to(BACKEND).with_suffix(".o")) for f in ADAPTERS if f.is_relative_to(BACKEND))
    makefile += ["plpgsql-reference: " + " ".join(objects),
                 '\t$(CC) $(CFLAGS) $(filter-out ' + exclude + ',$(call expand_subsys,$(OBJS))) $^ $(LDFLAGS) -Wl,--wrap=main,--wrap=CopyErrorData $(LIBS) -o "' + str(work / "probe") + '"']
    make = work / "Makefile"
    make.write_text("\n".join(makefile) + "\n")
    subprocess.run(["make", "-s", "-j6", "-C", build / BACKEND, "-f", make, "plpgsql-reference"], check=True)
    dest = run / "plpgsql-probe"
    (work / "probe").replace(dest)
    print(dest)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", type=Path)
    run = parser.parse_args().run.resolve()
    with tempfile.TemporaryDirectory(prefix="plpgsql-build-", dir=run) as work:
        build(run, Path(work))
