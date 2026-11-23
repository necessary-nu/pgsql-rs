#!/usr/bin/env ruby

# rubocop:disable Metrics/AbcSize, Metrics/LineLength, Metrics/MethodLength, Style/WordArray, Metrics/ClassLength, Style/Documentation, Metrics/CyclomaticComplexity, Metrics/PerceivedComplexity, Style/MutableConstant, Style/TrailingCommaInLiteral

require 'bundler'
require 'json'

class Generator
  def initialize
    @nodetypes = JSON.parse(File.read('./srcdata/nodetypes.json'))
    @struct_defs = JSON.parse(File.read('./srcdata/struct_defs.json'))
    @enum_defs = JSON.parse(File.read('./srcdata/enum_defs.json'))
    @typedefs = JSON.parse(File.read('./srcdata/typedefs.json'))
    @all_known_enums = JSON.parse(File.read('./srcdata/all_known_enums.json'))
  end

  FINGERPRINT_RES_TARGET_NAME = <<-EOL
  if (node->name != NULL && (field_name == NULL || parent == NULL || !IsA(parent, SelectStmt) || strcmp(field_name, "targetList") != 0)) {
    _fingerprintString(ctx, "name");
    _fingerprintString(ctx, node->name);
  }

  EOL

  FINGERPRINT_A_EXPR_KIND = <<-EOL
  if (true) {
    _fingerprintString(ctx, "kind");
    if (node->kind == AEXPR_OP_ANY || node->kind == AEXPR_IN)
      _fingerprintString(ctx, "AEXPR_OP");
    else
      _fingerprintString(ctx, _enumToStringA_Expr_Kind(node->kind));
  }

  EOL

  FINGERPRINT_NODE = <<-EOL
  if (true) {
    XXH3_state_t* prev = XXH3_createState();
    XXH64_hash_t hash;

    XXH3_copyState(prev, ctx->xxh_state);
    _fingerprintString(ctx, "%<name>s");

    hash = XXH3_64bits_digest(ctx->xxh_state);
    _fingerprintNode(ctx, %<cast>s&node->%<name>s, node, "%<name>s", depth + 1);
    if (hash == XXH3_64bits_digest(ctx->xxh_state)) {
      XXH3_copyState(ctx->xxh_state, prev);
      if (ctx->write_tokens)
        dlist_delete(dlist_tail_node(&ctx->tokens));
    }
    XXH3_freeState(prev);
  }

  EOL

  FINGERPRINT_NODE_PTR = <<-EOL
  if (node->%<name>s != NULL) {
    XXH3_state_t* prev = XXH3_createState();
    XXH64_hash_t hash;

    XXH3_copyState(prev, ctx->xxh_state);
    _fingerprintString(ctx, "%<name>s");

    hash = XXH3_64bits_digest(ctx->xxh_state);
    _fingerprintNode(ctx, %<cast>snode->%<name>s, node, "%<name>s", depth + 1);
    if (hash == XXH3_64bits_digest(ctx->xxh_state)) {
      XXH3_copyState(ctx->xxh_state, prev);
      if (ctx->write_tokens)
        dlist_delete(dlist_tail_node(&ctx->tokens));
    }
    XXH3_freeState(prev);
  }

  EOL

  # This recurses into the node type's function directly, bypassing the depth
  # cutoff in _fingerprintNode, so apply the same cutoff here (e.g. a long UNION
  # chain nests SelectStmt in SelectStmt without going through _fingerprintNode)
  FINGERPRINT_SPECIFIC_NODE_PTR = <<-EOL
  if (node->%<name>s != NULL && depth + 1 < 100) {
    XXH3_state_t* prev = XXH3_createState();
    XXH64_hash_t hash;

    XXH3_copyState(prev, ctx->xxh_state);
    _fingerprintString(ctx, "%<name>s");

    hash = XXH3_64bits_digest(ctx->xxh_state);
    _fingerprint%<typename>s(ctx, node->%<name>s, node, "%<name>s", depth + 1);
    if (hash == XXH3_64bits_digest(ctx->xxh_state)) {
      XXH3_copyState(ctx->xxh_state, prev);
      if (ctx->write_tokens)
        dlist_delete(dlist_tail_node(&ctx->tokens));
    }
    XXH3_freeState(prev);
  }

  EOL

  FINGERPRINT_LIST = <<-EOL
  if (node->%<name>s != NULL && node->%<name>s->length > 0) {
    XXH3_state_t* prev = XXH3_createState();
    XXH64_hash_t hash;

    XXH3_copyState(prev, ctx->xxh_state);
    _fingerprintString(ctx, "%<name>s");

    hash = XXH3_64bits_digest(ctx->xxh_state);
    _fingerprintNode(ctx, node->%<name>s, node, "%<name>s", depth + 1);
    if (hash == XXH3_64bits_digest(ctx->xxh_state) && !(list_length(node->%<name>s) == 1 && linitial(node->%<name>s) == NIL)) {
      XXH3_copyState(ctx->xxh_state, prev);
      if (ctx->write_tokens)
        dlist_delete(dlist_tail_node(&ctx->tokens));
    }
    XXH3_freeState(prev);
  }
  EOL

  FINGERPRINT_INT = <<-EOL
  if (node->%<name>s != 0) {
    char buffer[50];
    sprintf(buffer, "%%d", node->%<name>s);
    _fingerprintString(ctx, "%<name>s");
    _fingerprintString(ctx, buffer);
  }

  EOL

  FINGERPRINT_LONG_INT = <<-EOL
  if (node->%<name>s != 0) {
    char buffer[50];
    sprintf(buffer, "%%ld", node->%<name>s);
    _fingerprintString(ctx, "%<name>s");
    _fingerprintString(ctx, buffer);
  }

  EOL

  FINGERPRINT_UINT64 = <<-EOL
  if (node->%<name>s != 0) {
    char buffer[50];
    sprintf(buffer, UINT64_FORMAT, node->%<name>s);
    _fingerprintString(ctx, "%<name>s");
    _fingerprintString(ctx, buffer);
  }

  EOL

  FINGERPRINT_FLOAT = <<-EOL
  if (node->%<name>s != 0) {
    char buffer[50];
    sprintf(buffer, "%%f", node->%<name>s);
    _fingerprintString(ctx, "%<name>s");
    _fingerprintString(ctx, buffer);
  }

  EOL

  FINGERPRINT_CHAR = <<-EOL
  if (node->%<name>s != 0) {
    char buffer[2] = {node->%<name>s, '\\0'};
    _fingerprintString(ctx, "%<name>s");
    _fingerprintString(ctx, buffer);
  }

  EOL

  FINGERPRINT_CHAR_PTR = <<-EOL
  if (node->%<name>s != NULL) {
    _fingerprintString(ctx, "%<name>s");
    _fingerprintString(ctx, node->%<name>s);
  }

  EOL

  FINGERPRINT_STRING = <<-EOL
  if (strlen(node->%<name>s->sval) > 0) {
    _fingerprintString(ctx, "%<name>s");
    _fingerprintString(ctx, node->%<name>s->sval);
  }

  EOL

  FINGERPRINT_BOOL = <<-EOL
  if (node->%<name>s) {
    _fingerprintString(ctx, "%<name>s");
    _fingerprintString(ctx, "true");
  }

  EOL

  FINGERPRINT_INT_ARRAY = <<-EOL
  if (true) {
    int x = -1;
    Bitmapset	*bms = bms_copy(node->%<name>s);

    _fingerprintString(ctx, "%<name>s");

  	while ((x = bms_next_member(bms, x)) >= 0) {
      char buffer[50];
      sprintf(buffer, "%%d", x);
      _fingerprintString(ctx, buffer);
    }

    bms_free(bms);
  }

  EOL

  FINGERPRINT_ENUM = <<-EOL
  if (true) {
    _fingerprintString(ctx, "%<name>s");
    _fingerprintString(ctx, _enumToString%<typename>s(node->%<name>s));
  }

  EOL

  # Fingerprinting additional code to be inserted
  FINGERPRINT_SKIP_NODES = [
    'A_Const',
    'Alias',
    'ParamRef',
    'SetToDefault',
    'IntList',
    'OidList',
    'Null',
  ]
  # Node types with a custom fingerprint implementation in
  # src/pg_query_fingerprint.c. The generator only emits the declaration
  # and the dispatch case for these, not the function definition.
  FINGERPRINT_CUSTOM_NODES = [
    'RangeVar',
  ]
  FINGERPRINT_OVERRIDE_FIELDS = {
    [nil, 'location'] => :skip,
    [nil, 'arg_location'] => :skip,
    [nil, 'conninfo_location'] => :skip,
    [nil, 'payload_location'] => :skip,
    [nil, 'list_start'] => :skip,
    [nil, 'list_end'] => :skip,
    ['ResTarget', 'name'] => :res_target_name,
    ['A_Expr', 'kind'] => :a_expr_kind,
    ['A_Expr', 'rexpr_list_start'] => :skip,
    ['A_Expr', 'rexpr_list_end'] => :skip,
    ['PrepareStmt', 'name'] => :skip,
    ['ExecuteStmt', 'name'] => :skip,
    ['DeallocateStmt', 'name'] => :skip,
    ['TransactionStmt', 'gid'] => :skip,
    ['TransactionStmt', 'savepoint_name'] => :skip,
    ['CreateFunctionStmt', 'options'] => :skip,
    ['FunctionParameter', 'name'] => :skip,
    ['DoStmt', 'args'] => :skip,
    ['ListenStmt', 'conditionname'] => :skip,
    ['UnlistenStmt', 'conditionname'] => :skip,
    ['NotifyStmt', 'conditionname'] => :skip,
    ['NotifyStmt', 'payload'] => :skip,
    ['DeclareCursorStmt', 'portalname'] => :skip,
    ['FetchStmt', 'portalname'] => :skip,
    ['ClosePortalStmt', 'portalname'] => :skip,
    ['RawStmt', 'stmt_len'] => :skip,
    ['RawStmt', 'stmt_location'] => :skip,
    ['JsonTablePath', 'value'] => :skip,
    ['JsonTablePathSpec', 'name_location'] => :skip,
    ['JsonTablePathSpec', 'location'] => :skip,
    ['VariableSetStmt', 'jumble_args'] => :skip, # TODO: Should we be smarter about using this flag ourselves?
    ['CreateRoleStmt', 'role'] => :skip,
    ['RenameStmt', 'newname'] => :skip,
    ['RenameStmt', 'subname'] => :skip,
    ['RoleSpec', 'rolename'] => :skip,
  }
  INT_TYPES = ['bits32', 'uint32', 'int', 'int32', 'uint16', 'int16', 'Oid', 'Index', 'AttrNumber', 'SubTransactionId', 'RelFileNumber', 'ParseLoc']
  LONG_INT_TYPES = ['long']
  UINT64_TYPES = ['uint64', 'AclMode']
  INT_ARRAY_TYPES = ['Bitmapset*', 'Bitmapset', 'Relids']
  FLOAT_TYPES = ['Cost', 'double', 'Cardinality']

  IGNORE_FOR_GENERATOR = ['Integer', 'Float', 'String', 'BitString', 'List']

  def field_kind(type, name, field_type)
    override = FINGERPRINT_OVERRIDE_FIELDS[[type, name]] || FINGERPRINT_OVERRIDE_FIELDS[[nil, name]]
    return override if override

    case field_type
    when 'Node', 'JsonTablePlan' then :node
    when 'Node*', 'Expr*', 'JsonTablePlan*' then :node_ptr
    when 'List*' then :list
    when 'CreateStmt' then :embedded
    when 'char' then :char
    when 'char*' then :char_ptr
    when 'String*' then :string
    when 'bool' then :bool
    when 'Datum', 'void*', 'Expr', 'NodeTag' then :ignore
    when *INT_TYPES then :int
    when *LONG_INT_TYPES then :long_int
    when *UINT64_TYPES then :uint64
    when *INT_ARRAY_TYPES then :int_array
    when *FLOAT_TYPES then :float
    else
      return :specific_node_ptr if field_type.end_with?('*') && @nodetypes.include?(field_type.chop)
      return :enum if @all_known_enums.include?(field_type)
      raise "unmapped fingerprint field #{type}.#{name}: #{field_type}"
    end
  end

  def layout
    nodes = {}
    ['nodes/parsenodes', 'nodes/primnodes'].each do |group|
      @struct_defs[group].each do |type, struct_def|
        next unless struct_def['fields']
        next if IGNORE_FOR_GENERATOR.include?(type)
        nodes[type] = if FINGERPRINT_SKIP_NODES.include?(type)
          :skip
        elsif FINGERPRINT_CUSTOM_NODES.include?(type)
          :custom
        else
          fields = struct_def['fields'].select { |f| f['name'] }.sort_by { |f| f['name'] }
          fields.map { |f| [f['name'], f['c_type'], field_kind(type, f['name'], f['c_type'])] }
        end
      end
    end
    nodes
  end

  def field_writer(name, field_type, kind)
    case kind
    when :ignore then ''
    when :skip then format("  // Intentionally ignoring node->%s for fingerprinting\n\n", name)
    when :res_target_name then FINGERPRINT_RES_TARGET_NAME
    when :a_expr_kind then FINGERPRINT_A_EXPR_KIND
    when :embedded
      format("  _fingerprintString(ctx, \"%s\");\n", name) +
        format("  _fingerprintCreateStmt(ctx, (const CreateStmt*) &node->%s, node, \"%s\", depth);\n", name, name)
    else
      template = self.class.const_get("FINGERPRINT_#{kind.to_s.upcase}")
      format(template, name: name, typename: field_type.delete_suffix('*'),
             cast: field_type.start_with?('JsonTablePlan') ? '(Node*)' : '')
    end
  end

  def generate_fingerprint_defs!
    @fingerprint_defs = layout.transform_values do |fields|
      case fields
      when :skip then "  // Intentionally ignoring all fields for fingerprinting\n"
      when :custom then :custom
      else fields.map { |field| field_writer(*field) }.join
      end
    end
  end

  def generate!
    generate_fingerprint_defs!

    defs = ''
    conds = ''

    @nodetypes.each do |type|
      fingerprint_def = @fingerprint_defs[type]
      next unless fingerprint_def
      defs += format("static void _fingerprint%s(FingerprintContext *ctx, const %s *node, const void *parent, const char *field_name, unsigned int depth);\n", type, type)
    end
    defs += "\n\n"

    @nodetypes.each do |type|
      # next if IGNORE_LIST.include?(type)
      fingerprint_def = @fingerprint_defs[type]
      next unless fingerprint_def

      if fingerprint_def == :custom
        defs += format("// _fingerprint%s has a custom implementation, see pg_query_fingerprint.c\n\n", type)
      else
        defs += "static void\n"
        defs += format("_fingerprint%s(FingerprintContext *ctx, const %s *node, const void *parent, const char *field_name, unsigned int depth)\n", type, type)
        defs += "{\n"
        defs += fingerprint_def
        defs += "}\n"
        defs += "\n"
      end

      conds += format("case T_%s:\n", type)
      if FINGERPRINT_SKIP_NODES.include?(type)
        conds += format("  // Intentionally ignoring for fingerprinting\n")
      else
        conds += format("  if (!IsA(castNode(TypeCast, (void*) obj)->arg, A_Const) && !IsA(castNode(TypeCast, (void*) obj)->arg, ParamRef))\n  {\n") if type == 'TypeCast'
        conds += format("  _fingerprintString(ctx, \"%s\");\n", type)
        conds += format("  _fingerprint%s(ctx, obj, parent, field_name, depth);\n", type)
        conds += "  }\n" if type == 'TypeCast'
      end
      conds += "  break;\n"
    end

    File.write('./src/include/pg_query_fingerprint_defs.c', defs)
    File.write('./src/include/pg_query_fingerprint_conds.c', conds)
  end
end

if ARGV == ['--layout']
  puts JSON.generate(Generator.new.layout)
else
  Generator.new.generate!
end
