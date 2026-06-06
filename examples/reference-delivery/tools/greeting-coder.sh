#!/bin/sh
# Deterministic reference-delivery demo coder.
#
# The production local-git coding workspace (temper-coding-workspace) invokes
# this command with the prepared checkout as the working directory and the
# work-item context written to the file named by $TEMPER_CODING_WORKSPACE_CONTEXT.
# The command must leave a MEANINGFUL, non-bookkeeping product diff in the
# working tree; the workspace then commits the branch, pushes it, and the
# engineer worker opens the implementation PR through Temper.
#
# This stand-in implements the seeded "configurable banner greeting" intake
# deterministically so the LLM-backed example converges to a merged PR without a
# real coding agent. Bind your own coder by exporting TEMPER_CODING_WORKSPACE_ROOT
# and TEMPER_CODING_WORKSPACE_COMMAND before ./run.sh start; this script is only
# the default when neither is set and exactly one repository is configured.
#
# POSIX sh only. It must be deterministic: re-running it on a fresh checkout of
# the same code issue must reproduce the same diff so CI re-runs are stable.
set -eu

greeting=${REFERENCE_DELIVERY_GREETING:-Hello from the reference-delivery demo}

mkdir -p src
cat >src/banner.sh <<EOF
#!/bin/sh
# Print the configurable service banner greeting on startup.
#
# Implements the reference-delivery intake: a BANNER_GREETING setting whose value
# is printed on startup, defaulting to the current text when unset.
: "\${BANNER_GREETING:=${greeting}}"
printf '%s\n' "\$BANNER_GREETING"
EOF
chmod +x src/banner.sh
