// Standard BIP-39 test vectors from Trezor's reference implementation:
// https://github.com/trezor/python-mnemonic/blob/master/vectors.json
// All vectors use the passphrase "TREZOR".

use winterwallet_core::WinternitzKeypair;

fn parse_hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

struct Vector {
    entropy: &'static str,
    mnemonic: &'static str,
    seed: &'static str,
}

const VECTORS: &[Vector] = &[
    // 12 words / 128 bits
    Vector {
        entropy: "00000000000000000000000000000000",
        mnemonic: "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about",
        seed: "c55257c360c07c72029aebc1b53c05ed0362ada38ead3e3e9efa3708e53495531f09a6987599d18264c1e1c92f2cf141630c7a3c4ab7c81b2f001698e7463b04",
    },
    Vector {
        entropy: "7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f7f",
        mnemonic: "legal winner thank year wave sausage worth useful legal winner thank yellow",
        seed: "2e8905819b8723fe2c1d161860e5ee1830318dbf49a83bd451cfb8440c28bd6fa457fe1296106559a3c80937a1c1069be3a3a5bd381ee6260e8d9739fce1f607",
    },
    Vector {
        entropy: "80808080808080808080808080808080",
        mnemonic: "letter advice cage absurd amount doctor acoustic avoid letter advice cage above",
        seed: "d71de856f81a8acc65e6fc851a38d4d7ec216fd0796d0a6827a3ad6ed5511a30fa280f12eb2e47ed2ac03b5c462a0358d18d69fe4f985ec81778c1b370b652a8",
    },
    Vector {
        entropy: "ffffffffffffffffffffffffffffffff",
        mnemonic: "zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo zoo wrong",
        seed: "ac27495480225222079d7be181583751e86f571027b0497b5b5d11218e0a8a13332572917f0f8e5a589620c6f15b11c61dee327651a14c34e18231052e48c069",
    },
    // 18 words / 192 bits
    Vector {
        entropy: "808080808080808080808080808080808080808080808080",
        mnemonic: "letter advice cage absurd amount doctor acoustic avoid letter advice cage absurd amount doctor acoustic avoid letter always",
        seed: "107d7c02a5aa6f38c58083ff74f04c607c2d2c0ecc55501dadd72d025b751bc27fe913ffb796f841c49b1d33b610cf0e91d3aa239027f5e99fe4ce9e5088cd65",
    },
    // 24 words / 256 bits
    Vector {
        entropy: "0000000000000000000000000000000000000000000000000000000000000000",
        mnemonic: "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon art",
        seed: "bda85446c68413707090a52022edd26a1c9462295029f2e60cd7c4f2bbd3097170af7a4d73245cafa9c3cca8d561a7c3de6f5d4a10be8ed2a5e608d68f92fcc8",
    },
];

#[test]
fn seed_matches_trezor_vectors() {
    for v in VECTORS {
        let expected = parse_hex(v.seed);
        let computed = WinternitzKeypair::seed_with_passphrase(v.mnemonic, "TREZOR")
            .unwrap_or_else(|e| panic!("vector failed validation: {:?} ({})", e, v.mnemonic));
        assert_eq!(
            computed.as_slice(),
            expected.as_slice(),
            "seed mismatch for mnemonic: {}",
            v.mnemonic,
        );
    }
}

#[test]
fn generate_mnemonic_matches_trezor_vectors() {
    // generate_mnemonic only handles 32-byte (24-word) entropy.
    for v in VECTORS.iter().filter(|v| v.entropy.len() == 64) {
        let entropy = parse_hex(v.entropy);
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&entropy);

        let words = WinternitzKeypair::generate_mnemonic(bytes);
        let joined = words.join(" ");
        assert_eq!(
            joined, v.mnemonic,
            "mnemonic mismatch for entropy {}",
            v.entropy
        );
    }
}

#[test]
fn validate_accepts_all_trezor_vectors() {
    // from_mnemonic implicitly validates checksum.
    for v in VECTORS {
        WinternitzKeypair::from_mnemonic(v.mnemonic, 0)
            .unwrap_or_else(|e| panic!("validation failed for {}: {:?}", v.mnemonic, e));
    }
}
