/* PostgreSQL 19 psql statement boundaries, expressed as source byte offsets. */
#include "postgres_fe.h"
#include <limits.h>
#include "fe_utils/psqlscan.h"
#include "mb/pg_wchar.h"

static char *
variable(const char *name, PsqlScanQuoteType quote, void *seen)
{
	*(bool *) seen = true;
	return NULL;
}

static void
span(const char *kind, size_t start, size_t end)
{
	if (end > start)
		printf("%s %zu %zu\n", kind, start, end);
}

static void
statement(const char *source, size_t start, size_t end, PQExpBuffer sql,
		  bool has_variable, bool send)
{
	const char *kind;

	if (!sql->len)
		kind = "trivia";
	else if (!send)
		kind = "psql-buffer";
	else if (has_variable)
		kind = "psql-variable";
	else if (end - start < sql->len ||
			 memcmp(source + end - sql->len, sql->data, sql->len) != 0)
		kind = "psql-escape";
	else
		kind = "sql";
	span(kind, start, end);
}

static bool
command(const char *s, size_t len, const char *word)
{
	size_t n = strlen(word);

	return len >= n && memcmp(s, word, n) == 0 &&
		(len == n || strchr(" \t\r\n", s[n]) != NULL);
}

static size_t
line_end(const char *s, size_t start, size_t size)
{
	const char *p = memchr(s + start, '\n', size - start);

	return p ? (size_t) (p - s + 1) : size;
}

static size_t
copy_data(const char *s, size_t start, size_t size)
{
	size_t end;

	for (size_t i = start; i < size; i = end)
	{
		size_t len;
		end = line_end(s, i, size);
		len = end - i;
		while (len && (s[i + len - 1] == '\n' || s[i + len - 1] == '\r'))
			len--;
		if (len == 2 && memcmp(s + i, "\\.", 2) == 0)
		{
			span("copy-data", start, end);
			return end;
		}
	}
	span("unterminated-copy-data", start, size);
	return size;
}

static bool
meta_copy_stdin(const char *text, size_t len)
{
	static const PsqlScanCallbacks callbacks = {0};
	PsqlScanState scan = psql_scan_create(&callbacks);
	PQExpBuffer sql = createPQExpBuffer();
	promptStatus_t prompt;
	bool copy;

	psql_scan_setup(scan, text, len, PG_UTF8, true);
	psql_scan(scan, sql, &prompt);
	copy = psql_scan_count_copy_from_stdin(scan) > 0;
	psql_scan_destroy(scan);
	destroyPQExpBuffer(sql);
	return copy;
}

static void
boundaries(const char *s, size_t size)
{
	static const PsqlScanCallbacks callbacks = {variable};
	PsqlScanState scan = psql_scan_create(&callbacks);
	PQExpBuffer sql = createPQExpBuffer();
	bool has_variable = false;
	size_t start = 0;

	psql_scan_set_passthrough(scan, &has_variable);
	while (start < size)
	{
		size_t base = start;
		psql_scan_setup(scan, s + base, size - base, PG_UTF8, true);
		for (;;)
		{
			int line, offset;
			promptStatus_t prompt;
			PsqlScanResult result = psql_scan(scan, sql, &prompt);
			size_t end;
			int copies;

			psql_scan_get_location(scan, &line, &offset);
			end = base + offset;
			if (result == PSCAN_BACKSLASH)
			{
				size_t slash = end - 1;
				size_t next = line_end(s, slash, size);
				const char *meta = s + slash + 1;
				size_t len = next - slash - 1;
				bool send = command(meta, len, "g") || command(meta, len, "gset") ||
					command(meta, len, "gx") || command(meta, len, "gexec") ||
					command(meta, len, "crosstabview") || command(meta, len, "watch");
				statement(s, start, slash, sql, has_variable, send);
				span("psql-meta", slash, next);
				copies = send ? psql_scan_count_copy_from_stdin(scan) : 0;
				if (command(meta, len, "copy") && meta_copy_stdin(meta, len))
					copies++;
				start = next;
				while (copies-- > 0)
					start = copy_data(s, start, size);
				break;
			}
			if (result != PSCAN_SEMICOLON)
				end = size;
			statement(s, start, end, sql, has_variable, true);
			start = end;
			copies = psql_scan_count_copy_from_stdin(scan);
			if (copies && start < size)
			{
				size_t next = line_end(s, start, size);
				span("copy-line-tail", start, next);
				start = next;
				while (copies-- > 0)
					start = copy_data(s, start, size);
				break;
			}
			if (result != PSCAN_SEMICOLON)
				break;
			psql_scan_reset(scan);
			resetPQExpBuffer(sql);
			has_variable = false;
		}
		psql_scan_finish(scan);
		psql_scan_reset(scan);
		resetPQExpBuffer(sql);
		has_variable = false;
	}
	psql_scan_destroy(scan);
	destroyPQExpBuffer(sql);
}

int
main(void)
{
	char chunk[8192];
	size_t n;
	PQExpBuffer source = createPQExpBuffer();
	while ((n = fread(chunk, 1, sizeof(chunk), stdin)) > 0)
		appendBinaryPQExpBuffer(source, chunk, n);
	if (ferror(stdin) || PQExpBufferBroken(source) || source->len > INT_MAX)
		return 1;
	boundaries(source->data, source->len);
	destroyPQExpBuffer(source);
	return 0;
}
