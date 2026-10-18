//! A `user` step's literal password as Linux stores it: SHA-512 crypt (`$6$`, the glibc and
//! musl default, 5000 rounds), with a salt derived from the lab, the machine and the user, so the
//! same spec always renders the same line. The password is lab content (it lands in generated
//! files anyway); hashing keeps it out of `/etc/shadow` in clear.

use sha2::{Digest, Sha512};

/// The alphabet of crypt's base64.
const ALPHABET: &[u8; 64] = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

/// The rounds: crypt's default, so the hash names none.
const ROUNDS: usize = 5000;

/// The order SHA-512 crypt writes its digest's bytes in, three at a time.
const ORDER: [(usize, usize, usize); 21] = [
    (0, 21, 42),
    (22, 43, 1),
    (44, 2, 23),
    (3, 24, 45),
    (25, 46, 4),
    (47, 5, 26),
    (6, 27, 48),
    (28, 49, 7),
    (50, 8, 29),
    (9, 30, 51),
    (31, 52, 10),
    (53, 11, 32),
    (12, 33, 54),
    (34, 55, 13),
    (56, 14, 35),
    (15, 36, 57),
    (37, 58, 16),
    (59, 17, 38),
    (18, 39, 60),
    (40, 61, 19),
    (62, 20, 41),
];

/// The `$6$<salt>$<hash>` line of `password` for user `user` of machine `machine` in lab `lab`.
#[must_use]
pub(crate) fn hash(lab: &str, machine: &str, user: &str, password: &str) -> String {
    let seed = digest(&[lab.as_bytes(), b"\0", machine.as_bytes(), b"\0", user.as_bytes()]);
    let salt: String = seed.iter().take(16).map(|b| char::from(symbol(u32::from(*b)))).collect();
    sha512_crypt(password.as_bytes(), salt.as_bytes())
}

/// SHA-512 crypt of `password` with `salt` (at most 16 bytes are used), as Ulrich Drepper's
/// specification writes it.
fn sha512_crypt(password: &[u8], salt: &[u8]) -> String {
    let salt = salt.get(..16).unwrap_or(salt);
    let alternate = digest(&[password, salt, password]);
    let mut first = Sha512::new();
    first.update(password);
    first.update(salt);
    first.update(repeat(&alternate, password.len()));
    let mut bits = password.len();
    while bits > 0 {
        if bits & 1 == 1 {
            first.update(alternate);
        } else {
            first.update(password);
        }
        bits >>= 1;
    }
    let mut round: [u8; 64] = first.finalize().into();
    let p_bytes = repeat(&digest(&vec![password; password.len()]), password.len());
    let s_bytes = repeat(&digest(&vec![salt; 16 + usize::from(round[0])]), salt.len());
    for i in 0..ROUNDS {
        let mut next = Sha512::new();
        if i % 2 == 1 {
            next.update(&p_bytes);
        } else {
            next.update(round);
        }
        if i % 3 != 0 {
            next.update(&s_bytes);
        }
        if i % 7 != 0 {
            next.update(&p_bytes);
        }
        if i % 2 == 1 {
            next.update(round);
        } else {
            next.update(&p_bytes);
        }
        round = next.finalize().into();
    }
    let at = |i: usize| u32::from(round.get(i).copied().unwrap_or(0));
    let mut out = format!("$6${}$", String::from_utf8_lossy(salt));
    for (x, y, z) in ORDER {
        push(&mut out, (at(x) << 16) | (at(y) << 8) | at(z), 4);
    }
    push(&mut out, at(63), 2);
    out
}

/// The SHA-512 of `parts`, one after the other.
fn digest(parts: &[&[u8]]) -> [u8; 64] {
    let mut hasher = Sha512::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// `block` repeated, then cut, to `len` bytes.
fn repeat(block: &[u8; 64], len: usize) -> Vec<u8> {
    block.iter().copied().cycle().take(len).collect()
}

/// `count` symbols of `word`, six bits at a time from the lowest.
fn push(out: &mut String, mut word: u32, count: usize) {
    for _ in 0..count {
        out.push(char::from(symbol(word)));
        word >>= 6;
    }
}

/// The symbol of the lowest six bits of `bits`.
fn symbol(bits: u32) -> u8 {
    let i = usize::try_from(bits & 0x3f).unwrap_or(0);
    ALPHABET.get(i).copied().unwrap_or(b'.')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The specification's own test vectors.
    #[test]
    fn the_specifications_vectors_hash_alike() {
        assert_eq!(
            sha512_crypt(b"Hello world!", b"saltstring"),
            "$6$saltstring$svn8UoSVapNtMuq1ukKS4tPQd8iKwSMHWjl/O817G3uBnIFNjnQJuesI68u4OTLiBFdcbYEdFCoEOfaS35inz1"
        );
        // Checked against musl's crypt(3) (Python's `crypt` module, on Alpine).
        assert_eq!(
            sha512_crypt(b"we have a short salt string but not a short password", b"short"),
            "$6$short$qmfj2meTBr5G2EAGIJ4vjX7RpefsD4JzpEyTAeEUJdzdxlBS6pe8gdMHm5zFftaFSj/2p2bjBwyVS9ZhWpLZt."
        );
        assert_eq!(
            sha512_crypt(b"", b"abc"),
            "$6$abc$mJP3a6FyA8uCnzRtlnNypPwjnvpi5TP9qOrInzrfDmwxUQG38PkpCPdqfTb8JQfAngapMxeim4AZ..hSdRRzD."
        );
        // A salt longer than 16 bytes is cut.
        assert_eq!(
            sha512_crypt(b"This is just a test", b"toolongsaltstring"),
            "$6$toolongsaltstrin$lQ8jolhgVRVhY4b5pZKaysCLi0QBxGoNeKQzQ3glMhwllF7oGDZxUhx1yxdYcz/e1JSbq3y6JMxxl8audkUEm0"
        );
    }

    #[test]
    fn a_users_hash_is_the_same_for_the_same_lab_and_differs_by_user() {
        let alice = hash("lab", "web", "alice", "winter");
        assert_eq!(alice, hash("lab", "web", "alice", "winter"));
        assert_ne!(alice, hash("lab", "web", "bob", "winter"));
        assert!(alice.starts_with("$6$") && alice.len() == 3 + 16 + 1 + 86, "{alice}");
    }
}
