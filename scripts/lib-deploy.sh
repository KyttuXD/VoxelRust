#!/bin/sh
# Shared by scripts/deploy-server.sh and scripts/deploy-web.sh (sourced, not run).
#
# Settings come from the environment, and from deploy.conf in the repository's
# root (or the file named by VOXELRUST_DEPLOY_CONF): see deploy.conf.example.
# Nothing secret is kept in them: keys are files (the signing key, ssh keys),
# named by path. The environment wins over the file.
#
# Provides: say, die, run (does it, or only prints it under --dry-run),
# need_setting NAME, remote HOST COMMAND...

DRY=0

say() { echo "deploy: $*"; }
die() { echo "deploy: $*" >&2; exit 1; }

# run COMMAND ARGS...: print it, and do it unless this is a dry run.
run() {
  if [ "$DRY" = 1 ]; then
    echo "  [dry-run] $*"
  else
    echo "  \$ $*"
    "$@"
  fi
}

# load_conf: read deploy.conf without letting it override what the caller's
# environment already set.
CONF_VARS="GITHUB_REPO GIT_REMOTE GIT_BRANCH RELEASE_TAG SSH_OPTS SITE_HOST SITE_ROOT SITE_KEEP SITE_URL MIRROR_HOST MIRROR_DIR SERVER_HOST SERVER_DIR"
load_conf() {
  conf="${VOXELRUST_DEPLOY_CONF:-$ROOT/deploy.conf}"
  [ -f "$conf" ] || return 0
  for n in $CONF_VARS; do
    eval "if [ \"\${$n+x}\" = x ]; then keep_$n=\$$n; kept_$n=1; fi"
  done
  # shellcheck disable=SC1090
  . "$conf"
  for n in $CONF_VARS; do
    eval "if [ \"\${kept_$n:-}\" = 1 ]; then $n=\$keep_$n; fi"
  done
}

need_setting() { # need_setting NAME: stop with a message if it is empty
  eval "v=\${$1:-}"
  [ -n "$v" ] || die "$1 is not set (put it in deploy.conf, see deploy.conf.example, or the environment)"
}

# remote HOST COMMAND...: run a command over ssh (with SSH_OPTS), or print it.
remote() {
  host="$1"; shift
  if [ "$DRY" = 1 ]; then
    echo "  [dry-run] ssh ${SSH_OPTS:-} $host $*"
  else
    echo "  \$ ssh $host $*"
    # shellcheck disable=SC2086
    ssh ${SSH_OPTS:-} "$host" "$@"
  fi
}

# Defaults (after load_conf).
set_defaults() {
  GITHUB_REPO="${GITHUB_REPO:-KyttuXD/VoxelRust}"
  GIT_REMOTE="${GIT_REMOTE:-origin}"
  GIT_BRANCH="${GIT_BRANCH:-main}"
  RELEASE_TAG="${RELEASE_TAG:-nightly}"
  SSH_OPTS="${SSH_OPTS:-}"
  SITE_KEEP="${SITE_KEEP:-3}"
  export RSYNC_RSH="ssh ${SSH_OPTS}"
}

# Only plain characters in what ends up in a remote command line.
check_plain() { # check_plain NAME
  eval "v=\${$1:-}"
  case "$v" in
    *[!A-Za-z0-9._/@:~+-]*) die "$1 has a character that is not allowed here: $v" ;;
  esac
}
