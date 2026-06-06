#!/bin/sh
# Deterministic basic-delivery demo coder.
#
# The production local-git coding workspace (temper-coding-workspace) invokes
# this command with the prepared checkout as the working directory and the
# work-item context written to the file named by $TEMPER_CODING_WORKSPACE_CONTEXT.
# The command must leave a MEANINGFUL, non-bookkeeping product diff in the
# working tree; the workspace then commits the branch, pushes it, and the
# engineer worker opens the implementation PR through Temper.
#
# This stand-in implements the seeded dead-simple intake (a configurable banner
# greeting) deterministically so the engineer head path converges to a merged PR
# without a real coding agent — for an offline/CI smoke run. Select it with
# BASIC_DELIVERY_CODER=greeting. The produced src/banner.sh also passes the
# bundled config/ci.yml (every *.sh in the tree must parse).
#
# NOTE: this stand-in only backs the ENGINEER head path. The architect's
# triage_workspace must emit the `ready_code` verdict, which this script does not
# do — so the greeting coder converges the engineer step only when the issue is
# already a ready `code` issue. For the full unattended triage->code->PR->merge
# run use the default BASIC_DELIVERY_CODER=smith (a real LLM architect).
#
# POSIX sh only. It must be deterministic: re-running it on a fresh checkout of
# the same code issue must reproduce the same diff so CI re-runs are stable.
set -eu

greeting=${BASIC_DELIVERY_GREETING:-Hello from the basic-delivery demo}

mkdir -p src
cat >src/banner.sh <<EOF
#!/bin/sh
# Print the configurable service banner greeting on startup.
#
# Implements the basic-delivery intake: a BANNER_GREETING setting whose value is
# printed on startup, defaulting to the current text when unset.
: "\${BANNER_GREETING:=${greeting}}"
printf '%s\n' "\$BANNER_GREETING"
EOF
chmod +x src/banner.sh
