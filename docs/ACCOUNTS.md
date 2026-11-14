# Accounts

On a multiplayer server a name can have an **account**: a PIN of 4 or 6 digits.
Nobody can join under a name that has an account without its PIN. Accounts
belong to one server (the same name on another server is another matter), and
singleplayer has nothing to do with them. PINs are the first kind of account;
the files and the protocol already say which `kind` an account is, so a real
password can replace the PIN later.

## What this protects against, honestly

A PIN of 4 or 6 digits **stops casual impersonation**: a friend, or a stranger,
who simply joins under your name. That is what it is for, and it does that.

It does **not** stop:

- **Someone who can watch the connection.** The game's network traffic is not
  encrypted. The PIN itself is never sent (see "How a login works"), but the
  login exchange is, and someone who records it can try every PIN against it
  offline: 10 000 guesses for 4 digits, a million for 6, which is seconds to
  minutes on any computer. The lockouts below only slow down someone who has to
  ask the server for every guess.
- **The server's owner.** They run the program that sees what you send, and they
  can reset any PIN. Accounts protect you from other players, not from the owner.
- **Anyone who can read the server's accounts file or your saved PIN.** The server
  keeps a *verifier* made from the PIN (a salted, 20 000 times repeated SHA-256,
  never the PIN itself), but with a login of this design the verifier is what a
  login is checked against, so a copy of the file can be used to log in, and
  guessing the PIN from it is quick.
- **A PIN you chose badly or use elsewhere.** Use one that is only for this.

Real passwords, and an encrypted connection, will come later. Until then treat a
PIN as a lock on a garden gate.

## For players

- On a server with accounts, type `/register 4821 4821` (your own PIN, twice) to
  protect your name. Or just join: a server that requires accounts asks you to
  choose a PIN when your name has none.
- When your name has an account the MULTIPLAYER screen asks for the PIN: digits
  only, shown as dots, 4 or 6 of them. A switch **remember the PIN** keeps it in
  `server-pins.txt` in your data folder, **in plain text**: anyone who can read
  that file can log in as you on those servers. It is not hidden, because hiding
  it would be pretending. Leave the switch off on a shared computer. Delete the
  file (or a line of it), or type `/forgetpin` in the game, to forget.
- `/pin OLD NEW` changes the PIN. `/register`, `/pin` and `/forgetpin` are
  handled by the game itself: they are not sent as chat and mods' scripts do not
  see them.
- A PIN may not be a trivial one (`1234`, `0000`, `123456`, `4321`, `1212`...):
  the game says so politely and asks for another, unless the server allows them.
  The server cannot check this itself (it never sees the PIN), so a game that
  does not check could still send one; the games from this release do.
- **Wrong tries.** After 5 wrong PINs the name is locked for a minute, then two
  minutes, four, eight... never more than an hour. The same count is kept for
  your network address, so guessing at several names does not help either. The
  game tells you how many tries are left, and until what time (UTC) you are locked.
- **An older game** (protocol 13) can still join servers that do not require
  accounts, as a guest, with a name that has no account. For a name with an
  account it is told "this name has an account: update your game to log in".
- A name with an account cannot be used by a second person, and a guest who joins
  as a name that is taken or has an account gets a number added to it, as before.
  When you log in while an old connection of yours is still hanging, the old one
  is dropped.
- Nothing about accounts is shown to other players, except that an administrator
  has a star in `/list`.

## For server owners

Settings go in `settings.txt` in the server's data folder (or on the command line):

| Setting | Command line | Meaning |
|---|---|---|
| `require_accounts=optional` (default), `yes`, `no` | `--require-accounts MODE` | `optional`: anyone may play as a guest or register. `yes`: every player needs an account (a new name registers while joining; games older than this release are turned away, they cannot make one). `no`: accounts are not offered (nobody can register); names that already have an account, such as the administrators', still log in |
| `admins=KYTTU` | | names reserved for administrators, comma separated, any case |
| `allow_weak_pins=yes` | | lets players choose `1234` and the like (default `no`) |

The public server's `/var/lib/voxelrust/settings.txt` carries `admins=KYTTU`; the
default is no administrators, so a server you start has none until you say so.

**Administrators.** A reserved name cannot be registered by joining: whoever tries
is told "this name is reserved". The owner makes the account from the machine:

```
voxelrust --server --data /var/lib/voxelrust --set-pin KYTTU 482915
```

That writes the account (or changes its PIN) and exits; then the owner joins
normally and is asked for the PIN. A logged-in administrator has every permission
(`build`, `gamemode`, `commands`, `voice`), may use all the operators' commands
(`/ban`, `/kick`, `/op`, `/perm`...) and, alone, `/accounts` (the accounts, with who
is locked out; never any verifier) and `/resetpin NAME [PIN]` (a new PIN; without
one, six random digits that the answer shows to the administrator, who tells the
player privately; the player then uses `/pin`). `/resetpin` never works on an
administrator's account (not another administrator's, not one's own): a hijacked session
would otherwise keep the account for good. An administrator changes their PIN with
`/pin OLD NEW` (which needs the old one), the server's owner with `--set-pin`. Operators that exist today keep
working. When `admins` names someone, the first player to join no longer becomes
an operator automatically. The `--set-pin` PIN is checked like any other (and
`allow_weak_pins` applies); it is given on the command line, so it is in the
shell's history: change it with `/pin` afterwards.

**Files and lockouts.** The accounts file and the remembered PINs are written to a copy that is made
private before any data goes in it, then moved over the old file, so other users of the machine never
get to read them, not even for a moment. A lockout of an account is written to the file at once
(other changes within ten seconds), so it holds across a restart; the wrong tries of an
*address* are kept in memory only (a restart forgets them, the accounts' own lockouts stay).

**Where it is kept.** `<world>.server-accounts.txt` in the world folder, next to
`<world>.server-ops.txt`, readable by its owner only (on systems that have file
modes). One line per account:

```
name kind salt verifier rounds created fails level locked_until last_fail
```

`kind` is `pin`; `salt` is 16 random bytes and `verifier` 32 bytes, both hex;
`rounds` is how many times the PIN was hashed (20 000); the times are seconds since
1970; the last four are the record of wrong tries. It is written when something
changes (at most every 10 seconds) and when the server stops. The PIN is not in it.
Removing a line removes the account (the name is free again, and anyone may take it:
reserved names stay reserved).

**Limits the server keeps.** 5 wrong tries then a lockout that doubles each time
(1, 2, 4, 8, 16, 32, then 60 minutes), per account and per source address; wrong
tries are forgotten after a day without one, the doubling after a week; a right PIN
or an administrator's `/resetpin` clears an account's record. All logins together are limited
(12 at once, one more every two seconds): when the server is busy with logins the
player is told to try again, and nothing is counted against them. A player who does
not answer a login challenge within 15 seconds is dropped. An address may make 5
accounts an hour; a server keeps 10 000 accounts at most. Proofs are compared in
constant time. Names that are reserved or have accounts are never given to a guest.

## How a login works

The PIN never crosses the wire. The recipe (`net/accounts/keys.rs`):

```
key   = SHA-256 repeated 20000 times over (salt, PIN)     the verifier the server keeps
proof = HMAC-SHA-256(key, nonce || lower-case name)
```

1. The player's game says `Hello` as usual (protocol 14 or later).
2. If the name has an account the server answers with an `AuthChallenge`: the
   account's salt, the number of rounds, and a fresh random 16-byte nonce. (If the
   name is locked out it says so instead.)
3. The game turns the PIN into the key, answers with the proof (`AuthLogin`), and the
   server compares it with its own in constant time. Right: the player is let in
   (`Welcome`, then the usual). Wrong: they are told how many tries are left (or until
   when they are locked) and dropped.
4. **Registering**: the server chooses a salt (`AuthChallenge`, kind register), the
   game derives the key with it and sends the key (`AuthRegister`); the server stores
   it. A player in the game asks with `AuthAsk` first (`/register`). **Changing**:
   `AuthChange` carries the proof for the old PIN and the key of the new one.

A game refuses a server that asks for fewer than 1 000 or more than 200 000 rounds.
Random bytes come from the system's source (`/dev/urandom`, `RtlGenRandom`).
The protocol numbers are in [MULTIPLAYER.md](MULTIPLAYER.md).

## Where it lives in the code

`src/net/accounts/`: `pin.rs` (what a PIN may be), `keys.rs` (the recipe),
`strikes.rs` (lockouts), `store.rs` (the accounts and their file), `config.rs`
(settings), `state.rs` (a running server's state), `server_side.rs` (the server's half,
a child of `net/server.rs`), `client.rs` (the game's half, `/register`, `/pin`),
`remember.rs` (saved PINs), `persist.rs`, `set_pin.rs`; the screen is
`client/screens/pin.rs`; HMAC is in `util/crypto/hmac.rs`.
