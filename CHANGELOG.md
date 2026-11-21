# Changelog

## 0.2.3

- Let MCP hosts authorize writes against resolved calendar targets before mutation.

## 0.2.2

- Let MCP hosts observe structured tool results for auditing without changing client schemas.
- Preserve resolved account and calendar IDs on event mutation responses.
- Report rejected event updates and cancellations as failed MCP calls.

## 0.2.1

- Use a supported macOS runner for Apple Silicon release binaries.
- Check CLI outputs and exit codes against the pre-extraction fixtures.

## 0.2.0

- Add the reusable ncal-core use cases and the transport-independent ncal-mcp tools.
- Add stdio MCP with a read-only policy and export of local credentials as JSON.
- Make native credential storage optional for library consumers.
- MCP tool names and argument schemas are public API. Breaking changes require
  a minor version bump while ncal is in the 0.x series.
