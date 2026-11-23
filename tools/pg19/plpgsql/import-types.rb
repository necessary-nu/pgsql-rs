#!/usr/bin/env ruby
# frozen_string_literal: true

require_relative '../libpg_query/scripts/extract_pg_types'
require 'optparse'
require 'open3'

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
  %(    Builtin { oid: #{r[:oid]}, name: "#{r[:typname]}", kind: b'#{r[:typtype]}', array_oid: #{r[:typarray]}, collatable: #{r[:typcollation].to_i != 0} },)
end
content = "&[\n#{output.join("\n")}\n]\n"
source = File.join(ARGV[0], 'patched-source/src')
plerrcodes, status = Open3.capture2('perl', File.join(source, 'pl/plpgsql/src/generate-plerrcodes.pl'),
                                    File.join(source, 'backend/utils/errcodes.txt'))
abort 'could not generate PG19 PL/pgSQL error conditions' unless status.success?
conditions = plerrcodes.scan(/"([a-z_0-9]+)", ERRCODE_/).flatten.sort
abort 'missing condition names' unless conditions.size == plerrcodes.scan(/ERRCODE_/).size && !conditions.empty?
files = { 'types.in' => content, 'conditions.in' => "&[\n#{conditions.map { |name| %(    "#{name}",) }.join("\n")}\n]\n" }
files.each do |name, generated|
  dest = File.expand_path("../../../crates/pg19-plpgsql/data/#{name}", __dir__)
  if write
    File.write(dest, generated)
  elsif File.read(dest) != generated
    abort "PG19 catalog differs from #{dest}"
  end
end
