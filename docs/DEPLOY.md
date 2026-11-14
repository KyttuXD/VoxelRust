# Deploying: the public server and the website

How a release gets from the build machine to the public server and to
`voxelrust.games.kyttuslab.org`, and how both follow releases by themselves
within one to three minutes. Nothing here is run by the build or the tests;
you run it.

The pieces:

| Piece | Where it runs | What it does |
| --- | --- | --- |
| `scripts/deploy-server.sh` | your machine | builds and signs a release, pushes to GitHub, uploads the release assets, fills the mirror |
| `scripts/deploy-web.sh` | your machine | builds the browser version and swaps it onto the website atomically |
| the server's own updater (`src/client/server_update/`) | the game server | polls the signed release list, installs a new program, warns players, restarts |
| `scripts/server-supervisor.sh` (as `run.sh`) | the game server | starts the server, follows its restarts, rolls a bad update back |
| `scripts/site-updater.sh` | the website's server | pulls the web build when the release number changes |

Everything the machines trust is signed with the update key (Ed25519, see
[BUILDING.md](BUILDING.md#signing-the-release)); a mirror or a network in
between cannot make a server or the website install anything that was not
published by the key's owner.

## Settings

Copy `deploy.conf.example` to `deploy.conf` (ignored by git) and fill in the
hosts, users and paths, or set the same names in the environment (which
wins). There are no secrets in it: ssh uses your keys and agent, `gh` uses
its own login, the signing key is a file (`VOXELRUST_SIGN_KEY`). Every deploy
script has `--dry-run`: it prints each command and runs none (nothing is
built, pushed, uploaded or copied).

| Name | Meaning |
| --- | --- |
| `GITHUB_REPO`, `GIT_REMOTE`, `GIT_BRANCH`, `RELEASE_TAG` | the repository (`KyttuXD/VoxelRust`), remote, branch (`main`) and rolling release (`nightly`) |
| `SSH_OPTS` | extra options for ssh and rsync (a key, a port) |
| `SITE_HOST`, `SITE_ROOT`, `SITE_KEEP` | the website: `user@host`, the folder it is served from, how many releases to keep (3, at least 2) |
| `MIRROR_HOST`, `MIRROR_DIR` | where the signed files are also copied (empty host: no mirror) |
| `SERVER_HOST`, `SERVER_DIR` | for `deploy-server.sh --install` only |

## Publishing a release: `deploy-server.sh`

```sh
scripts/deploy-server.sh --dry-run        # look first
scripts/deploy-server.sh                  # asks you to type yes
scripts/deploy-server.sh --web            # and the website, at once
```

1. `scripts/package-release.sh` builds every version and writes the signed
   `nightly.txt` (it refuses without the signing key, or when the release
   number would go down). `--skip-build` uses `dist/nightly` as it is, if its
   commit is `HEAD`.
2. `git push` of `main`.
3. The release assets go to the `nightly` release with `gh release upload
   --clobber`: everything first, then `nightly.txt`, then `nightly.txt.sig`, so
   a list never names a file that is not there. The `nightly` tag is moved to
   `HEAD`.
4. The mirror folder on your server gets the same signed files (the list
   last): `scripts/publish-mirror.sh`. The repository is private, so the
   game's anonymous download cannot reach the release; the mirror can (see
   below).
5. `--install` also copies the server package (`scripts/package-server.sh`)
   to `SERVER_HOST:SERVER_DIR`: needed once, or to change `run.sh` or SDL2.
   Running servers update their own program from steps 3 or 4.

The release list `nightly.txt` has one extra line for the website,
`web voxelrust-web.zip <sha256> <bytes>`; the game ignores it.

## The public server

### Installing it

On the server machine (Linux x86-64):

```sh
tar -xzf voxelrust-server-linux-x86_64.tar.gz && cd voxelrust-server
./run.sh --world public --port 25590 --data ./data
```

and in `data/settings.txt`, so it reads releases from the mirror (needed while
the repository is private, and it also saves GitHub traffic):

```
update_mirror=https://voxelrust.games.kyttuslab.org/updates/
update_check_seconds=90
```

A systemd unit (`/etc/systemd/system/voxelrust.service`):

```ini
[Unit]
Description=VoxelRust server
After=network-online.target
Wants=network-online.target

[Service]
User=voxelrust
WorkingDirectory=/srv/voxelrust/voxelrust-server
ExecStart=/srv/voxelrust/voxelrust-server/run.sh --world public --port 25590 --data /srv/voxelrust/data --log /srv/voxelrust/server.log
# run.sh is the supervisor: it ends only when the server is stopped on purpose.
Restart=on-failure
RestartSec=10
KillMode=mixed
TimeoutStopSec=30

[Install]
WantedBy=multi-user.target
```

`run.sh` is `scripts/server-supervisor.sh`. It starts the server and keeps it
running through updates; the unit only has to keep `run.sh` itself up.

### How it notices a release (1 to 3 minutes)

A thread in the server (`server_update::watch`) reads the signed
`nightly.txt` and its signature every `update_check_seconds` seconds, from the
mirror if `update_mirror` is set, then from the GitHub release. The two files
are a few hundred bytes, so a plain request is cheaper than any bookkeeping;
the signature is checked before a number in the list is believed. Only when
the release number (`seq`) is **larger** than the program's own (and larger
than a release that was rolled back, see below) does the full path run:
download the program, check its SHA-256 and size, run it with `--stamp`, and
put it in place of the running program.

| Setting (`settings.txt` in the `--data` folder) | Meaning |
| --- | --- |
| `update_check_seconds` | seconds between looks: default **90**, clamped to 30..600. A sixth of it is added at random, so servers do not all ask at once. A release is therefore picked up within about 1 to 3 minutes (the interval, plus the download). |
| `update_mirror` | where to look first (see [BUILDING.md](BUILDING.md#a-plain-http-mirror-for-the-old-mac-build)) |
| `require_accounts`, `admins`, `allow_weak_pins` | accounts and administrators (see [ACCOUNTS.md](ACCOUNTS.md)). The public server's file gets `admins=KYTTU`; its account is made once with `voxelrust --server --data /var/lib/voxelrust --set-pin KYTTU PIN` |

After failures in a row (unreadable list, a refused signature, a download that
does not match, a program that does not run) it waits twice as long each time,
up to 30 minutes, and goes back to the interval after a success. `--no-update`
turns it all off.

### The restart

When a new program is in place, the server tells every player in chat
**RESTARTING FOR AN UPDATE IN 30 SECONDS** (and again at 10 seconds), keeps
playing, then saves the world and exits with code 75, which `run.sh`
answers by starting the new program. With nobody connected it goes at once.
Players reconnect to the same address and port. A server started by an
earlier update restarts for the next one too.

Without the supervisor (the program started directly, not through
`run.sh`), the server starts the new program itself, as before, and there is
no rollback.

### Rollback and crash loops

`run.sh` keeps a copy of the program as it was when it started, and calls it
the known-good one once it has run for 20 seconds.

- If a program installed by an update **stops with an error within 20
  seconds**, `run.sh` puts the previous program back, notes the bad release
  in `<run.sh's folder>/supervisor/rejected-seq` and keeps the bad program as
  `supervisor/rejected-program`. The server reads that number and does not
  fetch that release again; the next, larger release is tried as usual.
- A server that crashes at any other time is started again after 5 seconds,
  then 10, 20, 40, then every 60; a run of a minute resets the wait.
- Exit code 0 (Ctrl+C, SIGTERM, `systemctl stop`) ends `run.sh` as well.

`VOXELRUST_STATE_DIR`, `VOXELRUST_ROLLBACK_SECONDS` and `VOXELRUST_DELAY_UNIT`
change where it keeps its notes, the 20 seconds, and the 5-second unit of the
waits. `scripts/test-supervisor.sh` checks the supervisor with stand-in
programs (no build needed). The rules themselves (interval clamp, release
compare, back-off, rollback decision) are unit tests in
`src/client/server_update/schedule.rs`.

## The website

The web server serves `SITE_ROOT/current`, a symlink into
`SITE_ROOT/releases/<release number>-<commit>/`. A release is uploaded to a
new folder and the symlink is flipped in one step (`ln -s` to a temporary name,
then `mv -T`), so a visitor never sees a half-written site and the old release
is still there for a rollback. Each release folder has `version.txt` (commit and
release number).

nginx, for example:

```nginx
server {
    server_name voxelrust.games.kyttuslab.org;
    root /var/www/voxelrust.games.kyttuslab.org/current;
    index index.html;
    gzip on;
    gzip_types application/wasm application/javascript;
    # The page and game files change with each release: always revalidate.
    add_header Cache-Control "no-cache";
    # The mirror the game servers and old Macs update from (plain files).
    location /updates/ { alias /var/www/voxelrust-updates/; }
    # (listen / TLS lines as usual)
}
```

### Pushing by hand: `deploy-web.sh`

```sh
scripts/deploy-web.sh --dry-run
scripts/deploy-web.sh                 # builds dist/web, uploads, flips
scripts/deploy-web.sh --skip-build    # use dist/web as it is
```

It uploads with `rsync` to `releases/<name>/`, flips `current`, writes
`current-seq`, and removes releases beyond the newest `SITE_KEEP`. It will not
put an older release number on the site unless `--force`.

### Following releases by itself: `site-updater.sh`

`scripts/site-updater.sh` runs **on the website's server**. Each run
fetches `nightly.txt` and `nightly.txt.sig` from `SITE_SOURCE`, checks the
signature with the public key, stops if the release number is not larger than
`current-seq` (two tiny requests, nothing else), and otherwise downloads the
`web` file the list names, checks its size and SHA-256, unpacks it into a new
release folder, flips `current`, and prunes. After a failure it waits 1, 2,
4... minutes (at most 30). One run at a time; `--dry-run` fetches and
verifies the list and says what it would do, changing nothing.

Set it up once on the server:

```sh
# on your machine: the public key (not secret) for the server
sh scripts/export-public-key.sh | ssh you@vps 'sudo mkdir -p /etc/voxelrust && sudo tee /etc/voxelrust/update-key.pub.pem >/dev/null'
# copy the script
scp scripts/site-updater.sh you@vps:/usr/local/bin/voxelrust-site-updater
```

`/etc/voxelrust/site-updater.conf` (no secrets):

```sh
SITE_ROOT=/var/www/voxelrust.games.kyttuslab.org
# The mirror folder that deploy-server.sh fills. A folder on the same machine
# needs no web request at all; a URL works too (the GitHub release folder once
# the repository is public).
SITE_SOURCE=/var/www/voxelrust-updates
SITE_PUBKEY=/etc/voxelrust/update-key.pub.pem
SITE_KEEP=3
```

Then either a systemd timer, which checks every minute (the 1 to 3 minute
target is the timer plus the download):

```ini
# /etc/systemd/system/voxelrust-site.service
[Unit]
Description=Update the VoxelRust website from the release

[Service]
Type=oneshot
User=www-data
ExecStart=/usr/local/bin/voxelrust-site-updater --once
```

```ini
# /etc/systemd/system/voxelrust-site.timer
[Unit]
Description=Check for a new VoxelRust web release every minute

[Timer]
OnBootSec=1min
OnUnitActiveSec=60s
AccuracySec=5s
RandomizedDelaySec=10s

[Install]
WantedBy=timers.target
```

or cron: `* * * * * www-data /usr/local/bin/voxelrust-site-updater --once >>/var/log/voxelrust-site.log 2>&1`.
(Or run it as a service with `--loop`, which repeats every
`SITE_CHECK_SECONDS`, default 90.) Needs `curl` (for a URL source), `openssl`,
`sha256sum` and `unzip` (or `python3`) on the server. The user it runs as must
own `SITE_ROOT`.

## Things not tested

These scripts were written without running a deploy: `deploy-server.sh` and
`deploy-web.sh` have only been run with `--dry-run`; `site-updater.sh` and
`server-supervisor.sh` are tested against local folders and stand-in programs
(`scripts/test-site-updater.sh`, `scripts/test-supervisor.sh`); the server's
polling thread, warning, and restart need a real run (a build with
`VOXELRUST_SEQ` set, a signed list and a mirror: see
`scripts/test-update.sh` for the pattern).
