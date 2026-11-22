//! Files a site stores encrypted (MEGA): the transfer decrypts every byte where it lands, so
//! the part file only ever holds the plain file. The cipher must be seekable, since segments
//! arrive in any order and a resume starts mid-file.

use aes::cipher::{KeyIvInit, StreamCipher, StreamCipherSeek};

type Aes128Ctr = ctr::Ctr128BE<aes::Aes128>;

/// How the bytes the server sends are encrypted.
#[derive(Clone, PartialEq, Eq)]
pub enum Decrypt {
    /// AES-128 in counter mode, the 128-bit counter big-endian, starting at `iv` for byte 0.
    Aes128Ctr { key: [u8; 16], iv: [u8; 16] },
}

impl std::fmt::Debug for Decrypt {
    // Never the key: tasks and sources end up in logs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Decrypt::Aes128Ctr { .. } => f.write_str("Aes128Ctr"),
        }
    }
}

impl Decrypt {
    /// Decrypt `data` in place, `offset` being where it starts in the file.
    pub fn apply(&self, offset: u64, data: &mut [u8]) {
        match self {
            Decrypt::Aes128Ctr { key, iv } => {
                let mut cipher = Aes128Ctr::new(key.into(), iv.into());
                cipher.seek(offset);
                cipher.apply_keystream(data);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; 16] = *b"0123456789abcdef";
    const IV: [u8; 16] = [7, 7, 7, 7, 7, 7, 7, 7, 0, 0, 0, 0, 0, 0, 0, 0];

    fn encrypt(plain: &[u8]) -> Vec<u8> {
        let mut data = plain.to_vec();
        Aes128Ctr::new(&KEY.into(), &IV.into()).apply_keystream(&mut data);
        data
    }

    #[test]
    fn any_piece_decrypts_on_its_own() {
        let plain: Vec<u8> = (0..10_000u32).map(|i| (i * 31 % 251) as u8).collect();
        let sealed = encrypt(&plain);
        let decrypt = Decrypt::Aes128Ctr { key: KEY, iv: IV };
        // Pieces not on a 16-byte boundary, in any order.
        for (start, end) in [(5000, 10_000), (0, 17), (17, 5000)] {
            let mut piece = sealed[start..end].to_vec();
            decrypt.apply(start as u64, &mut piece);
            assert_eq!(piece, plain[start..end]);
        }
    }

    /// AES-128-CTR test vector, NIST SP 800-38A F.5.1.
    #[test]
    fn matches_the_standard() {
        let hex = |s: &str| (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect::<Vec<u8>>();
        let key: [u8; 16] = hex("2b7e151628aed2a6abf7158809cf4f3c").try_into().unwrap();
        let iv: [u8; 16] = hex("f0f1f2f3f4f5f6f7f8f9fafbfcfdfeff").try_into().unwrap();
        let mut block = hex("874d6191b620e3261bef6864990db6ce");
        Decrypt::Aes128Ctr { key, iv }.apply(0, &mut block);
        assert_eq!(block, hex("6bc1bee22e409f96e93d7e117393172a"));
    }
}
