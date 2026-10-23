# Optional artifact storage

`suite/artifacts.py` supports SHA256-addressed payloads outside the repository.
The default store is `~/Library/Application Support/keel/artifacts` on macOS and
`~/.local/share/keel/artifacts` elsewhere; `KEEL_ARTIFACT_STORE` selects another
local store. No remote artifact backend is configured.

Normal builds do not require external proof payloads. A byte-identical restored
payload is not evidence of a fresh test execution.
