#!/bin/sh
# Deterministic basic-delivery workflow-role-decision responder.
#
# Reads one WorkflowRoleDecisionRequest JSON value from stdin and emits one compact
# WorkflowRoleDecisionReply JSON object. This intentionally uses tiny text
# matching instead of jq so the basic-delivery smoke can run without provider
# credentials, network access, or non-POSIX dependencies.
set -eu

request=$(cat)

authorized() {
	case "$request" in
		*'"authorized_actions"'*'"action"'*'"'$1'"'*) return 0 ;;
		*) return 1 ;;
	esac
}

engineer_repair_context() {
	case "$request" in
		*'"role"'*'"engineer"'*'"address_ci_failure"'*) return 0 ;;
		*'"queue"'*'"code_ready"'*'"address_ci_failure"'*) return 0 ;;
		*'"queue"'*'"ci_failed"'*'"address_ci_failure"'*) return 0 ;;
		*'"artifact"'*'"type"'*'"pull_request"'*'"address_ci_failure"'*) return 0 ;;
		*) return 1 ;;
	esac
}

if authorized triage_intake; then
	action=triage_intake
	reason='deterministic greeting smoke selects intake triage'
elif authorized open_pr; then
	action=open_pr
	reason='deterministic greeting smoke selects implementation PR'
elif authorized address_ci_failure && engineer_repair_context; then
	action=address_ci_failure
	reason='deterministic greeting smoke selects engineer CI repair'
else
	action=no_action
	reason='deterministic greeting smoke has no safe authorized action'
fi

printf '{"protocol_version":1,"action":"%s","reason":"%s"}\n' "$action" "$reason"
