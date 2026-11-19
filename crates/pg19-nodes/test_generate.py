import unittest

from generate import emit_enum, field_name, rust_type


class DeclarationsTest(unittest.TestCase):
    def test_nullable_nodes_and_embedded_structs(self):
        records = {"CreateStmt", "Query", "SelectStmt"}
        expected = {
            "CreateStmt": "CreateStmt",
            "Query *": "Option<Box<Query>>",
            "Node *": "Option<Box<Node>>",
            "List *": "Vec<Node>",
            "String *": "Option<String>",
            "char *": "Option<String>",
            "A_Const *": "Option<Box<AConst>>",
        }
        for ctype, output in expected.items():
            with self.subTest(ctype=ctype):
                field = {"type": {"qualType": ctype}, "name": "child"}
                self.assertEqual(rust_type(field, {}, records), output)

    def test_unmapped_field_requires_review(self):
        field = {"type": {"qualType": "UpstreamAddition *"}, "name": "value"}
        with self.assertRaisesRegex(ValueError, "value: UpstreamAddition"):
            rust_type(field, {}, set())

    def test_enum_values_follow_c(self):
        enum = {"inner": [
            {"name": "FIRST", "kind": "EnumConstantDecl"},
            {"name": "ASCII", "kind": "EnumConstantDecl",
             "inner": [{"kind": "ConstantExpr", "value": "97"}]},
            {"name": "NEXT", "kind": "EnumConstantDecl"},
        ]}
        self.assertEqual(emit_enum("Kind", enum)[1:-1], [
            "        FIRST = 0,", "        ASCII = 97,", "        NEXT = 98,",
        ])

    def test_preserves_keywords_and_acronyms(self):
        self.assertEqual(field_name("where"), "r#where")
        self.assertEqual(field_name("XMLType"), "xml_type")


if __name__ == "__main__":
    unittest.main()
