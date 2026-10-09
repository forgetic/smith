These are the original JSON lines written by smith at `30259d8`, under
`calls` capture, against Skein's fake TLS provider. The manifest records
hashes, exact retained lines and the offline recording runs. No line is
rewritten. The traces contain no real credentials; fake grants are scanned
out too. The baseline's immutable release arm remains unchanged.

The first stream is the shipped local/agent answer story. The second is the
first-sign-in and checked-commit story, using only the fake issuer and
browser. A temporary test-only `Files::drop` hook retained the scratch
folders after the stories; it changed no product source and was restored
in the private clone afterward. The scratches remain outside repositories.

The trace does not retain every final shutdown fact. Missing facts and loss
counts are unavailable, never evidence that no fact was lost. The `Used`
facts carry cumulative-domain evidence, but this calls-only capture is not
used for per-response token accounting. Everything capture is recorded by
the later live-adapter increment.
