//! What a running server keeps for accounts: the settings, the accounts, the limits on
//! login attempts, and what each connection is in the middle of.

use super::config::Config;
use super::keys::{Nonce, Salt};
use super::rng::Rng;
use super::store::Accounts;
use super::strikes::Strikes;
use alloc::collections::BTreeMap;
use alloc::string::String;

/// Logins (right or wrong) the whole server takes at once, and how many more come each second:
/// a flood from anywhere runs into this whatever addresses it comes from.
const BURST: f32 = 12.0;
const PER_SECOND: f32 = 0.5;
/// Most addresses remembered at a time.
const MAX_ADDRESSES: usize = 2048;
/// Accounts one address may make in an hour.
pub const MAX_REGISTRATIONS_PER_HOUR: u32 = 5;
/// Seconds a player has to answer a login challenge when joining.
pub const ANSWER_SECONDS: f32 = 15.0;
/// Seconds an in-game challenge (for /register or /pin) stays good.
pub const INGAME_SECONDS: f32 = 30.0;

/// What a login challenge is for (the numbers are the protocol's).
pub mod op {
    pub const LOGIN: u8 = 1;
    pub const REGISTER: u8 = 2;
    pub const CHANGE: u8 = 3;
}

/// Bits of the flags in [`crate::proto::Msg::Account`] and [`crate::proto::Msg::AuthChallenge`].
pub mod flag {
    /// This connection is logged in as an account.
    pub const LOGGED_IN: u8 = 1;
    /// ... and the account is an administrator's.
    pub const ADMIN: u8 = 2;
    /// The server lets players choose trivial PINs.
    pub const WEAK_OK: u8 = 4;
}

/// A challenge sent and waiting for its answer.
#[derive(Clone, Debug)]
pub struct Pending {
    pub op: u8,
    /// The account's name, lower case.
    pub name: String,
    /// The name as the player typed it (what others will see).
    pub shown: String,
    pub nonce: Nonce,
    /// The salt of the new PIN's key (register, change).
    pub new_salt: Salt,
}

/// Per connection.
#[derive(Default)]
pub struct ConnAuth {
    /// The protocol version the client said it speaks (0 before it said hello).
    pub version: u16,
    pub ip: Option<[u8; 4]>,
    /// The account this connection is logged in as (lower case).
    pub account: Option<String>,
    pub pending: Option<Pending>,
    /// Seconds the pending challenge has waited.
    pub waited: f32,
    /// Seconds since this connection last asked for a challenge.
    pub ask_gap: f32,
}

#[derive(Default)]
pub struct AddrRecord {
    pub strikes: Strikes,
    pub registrations: u32,
    pub reg_since: u64,
}

/// A bucket of login attempts that fills up slowly.
pub struct Bucket {
    tokens: f32,
}

impl Default for Bucket {
    fn default() -> Bucket {
        Bucket { tokens: BURST }
    }
}

impl Bucket {
    /// A bucket with nothing in it (tests).
    pub fn empty() -> Bucket {
        Bucket { tokens: 0.0 }
    }

    pub fn refill(&mut self, dt: f32) {
        self.tokens = (self.tokens + dt.clamp(0.0, 1.0) * PER_SECOND).min(BURST);
    }

    /// Take one attempt, if there is one.
    pub fn take(&mut self) -> bool {
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[derive(Default)]
pub struct AccountState {
    pub cfg: Config,
    pub accounts: Accounts,
    pub rng: Rng,
    pub global: Bucket,
    pub addrs: BTreeMap<[u8; 4], AddrRecord>,
    /// Seconds until the accounts file may be written again.
    pub write_wait: f32,
    /// Seconds added to the clock (tests move time forward with it).
    pub skew: u64,
}

impl AccountState {
    /// The record of an address (made if need be, unless too many are kept already).
    pub fn addr_mut(&mut self, ip: [u8; 4], now: u64) -> Option<&mut AddrRecord> {
        if !self.addrs.contains_key(&ip) && self.addrs.len() >= MAX_ADDRESSES {
            self.addrs.retain(|_, r| !(r.strikes.is_clear(now) && now.saturating_sub(r.reg_since) > 3600));
            if self.addrs.len() >= MAX_ADDRESSES {
                return None;
            }
        }
        Some(self.addrs.entry(ip).or_default())
    }

    /// Until when the name or the address is locked out, if either is.
    pub fn locked_until(&self, name: &str, ip: Option<[u8; 4]>, now: u64) -> Option<u64> {
        let by_name = self.accounts.get(name).and_then(|a| a.strikes.locked(now));
        let by_addr = ip.and_then(|ip| self.addrs.get(&ip)).and_then(|r| r.strikes.locked(now));
        match (by_name, by_addr) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }

    /// A wrong PIN for `name` from `ip` at `now`: both records are charged. Returns what the
    /// player is told: tries left, or until when they are locked.
    pub fn charge_failure(&mut self, name: &str, ip: Option<[u8; 4]>, now: u64) -> super::strikes::Fail {
        use super::strikes::Fail;
        let by_name = self.accounts.get_mut(name).map(|a| a.strikes.fail(now));
        self.accounts.dirty |= by_name.is_some();
        let by_addr = ip.and_then(|ip| self.addr_mut(ip, now)).map(|r| r.strikes.fail(now));
        match (by_name, by_addr) {
            (Some(Fail::Locked(a)), Some(Fail::Locked(b))) => Fail::Locked(a.max(b)),
            (Some(Fail::Locked(a)), _) | (_, Some(Fail::Locked(a))) => Fail::Locked(a),
            (Some(Fail::Left(a)), Some(Fail::Left(b))) => Fail::Left(a.min(b)),
            (Some(f), None) | (None, Some(f)) => f,
            (None, None) => Fail::Left(1),
        }
    }

    /// Whether this address may make another account now (and count it if so).
    pub fn allow_registration(&mut self, ip: Option<[u8; 4]>, now: u64) -> bool {
        let Some(ip) = ip else { return true };
        let Some(r) = self.addr_mut(ip, now) else { return false };
        if now.saturating_sub(r.reg_since) > 3600 {
            r.reg_since = now;
            r.registrations = 0;
        }
        if r.registrations >= MAX_REGISTRATIONS_PER_HOUR {
            return false;
        }
        r.registrations += 1;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::accounts::strikes::Fail;

    #[test]
    fn the_bucket_runs_dry_and_fills_again() {
        let mut b = Bucket::default();
        let taken = (0..100).filter(|_| b.take()).count();
        assert_eq!(taken, BURST as usize);
        for _ in 0..8 {
            b.refill(1.0);
        }
        assert!(b.take(), "eight seconds bring back four");
        assert_eq!((0..10).filter(|_| b.take()).count(), 3);
    }

    #[test]
    fn an_address_is_locked_by_wrong_tries_on_any_account() {
        let mut s = AccountState::default();
        let ip = Some([10, 0, 0, 7]);
        let now = 5_000;
        for k in 1..5 {
            // (No account of these names: only the address keeps count.)
            assert_eq!(s.charge_failure(&alloc::format!("n{k}"), ip, now), Fail::Left(5 - k));
        }
        assert_eq!(s.charge_failure("n9", ip, now), Fail::Locked(now + 60));
        assert_eq!(s.locked_until("whoever", ip, now + 1), Some(now + 60));
        assert_eq!(s.locked_until("whoever", Some([10, 0, 0, 8]), now + 1), None);
        assert_eq!(s.locked_until("whoever", ip, now + 61), None);
    }

    #[test]
    fn registrations_per_address_are_limited_by_the_hour() {
        let mut s = AccountState::default();
        let ip = Some([1, 2, 3, 4]);
        for _ in 0..MAX_REGISTRATIONS_PER_HOUR {
            assert!(s.allow_registration(ip, 100));
        }
        assert!(!s.allow_registration(ip, 200));
        assert!(s.allow_registration(Some([1, 2, 3, 5]), 200));
        assert!(s.allow_registration(ip, 100 + 3601));
        // An address nobody could read is not limited (nothing to key it on).
        assert!(s.allow_registration(None, 0));
    }
}
