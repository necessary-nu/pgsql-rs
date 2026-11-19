#!/usr/bin/env ruby
# frozen_string_literal: true

require_relative '../../../scripts/extract_pg_types'
require 'optparse'

write = false
OptionParser.new { |opts| opts.on('--write') { write = true } }.parse!
abort 'expected a completed PG19 reference import' unless ARGV.length == 1
include_dir = File.join(ARGV[0], 'patched/src/include')
abort 'expected PG19.0' unless File.readlines(File.join(include_dir, 'pg_config.h')).any? { |line| line.strip == '#define PG_VERSION_NUM 190000' }
rows = extract_pg_types(File.join(include_dir, 'catalog/postgres.bki')).sort_by { |r| r[:typname] }
abort 'duplicate built-in type names' unless rows.map { |row| row[:typname] }.uniq.size == rows.size
output = rows.map do |r|
  unless r[:typnamespace] == 11 && r[:typname].match?(/\A[a-z_0-9]+\z/) && r[:typtype].match?(/\A[a-z]\z/)
    abort "unsupported pg_type row #{r[:oid]}"
  end
  %(    Builtin { oid: #{r[:oid]}, name: "#{r[:typname]}", kind: b'#{r[:typtype]}', array_oid: #{r[:typarray]} },)
end
content = "&[\n#{output.join("\n")}\n]\n"
dest = File.expand_path('../../../crates/pg19-plpgsql/data/types.in', __dir__)
if write
  File.write(dest, content)
elsif File.read(dest) != content
  abort "PG19 pg_type differs from #{dest}"
end
