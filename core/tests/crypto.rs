use winterwallet_core::{
    WinternitzKeypair, WinternitzPrivkey, WinternitzPubkey, WinternitzRoot, WinternitzSignature,
};

const MNEMONIC: &str =
    "earn foster affair make exclude object spring oppose one hollow garage kind";

const N: usize = 16;

fn keypair() -> WinternitzKeypair {
    WinternitzKeypair::from_mnemonic(MNEMONIC, 0).expect("valid mnemonic")
}

#[test]
fn dump_pubkey_and_root() {
    // Run: cargo test --test crypto dump_pubkey_and_root -- --nocapture
    let sk: WinternitzPrivkey<N> = keypair().derive();
    let pk: WinternitzPubkey<N> = sk.to_pubkey();
    let root: WinternitzRoot = pk.merklize();
    println!("{:?}", pk);
    println!("{}", root);
}

// Canonical Merkle root for MNEMONIC at (wallet=0, parent=0, child=0), N=16.
// Tightly couples mnemonic → seed → master → privkey → pubkey → root.
// If any link in that chain drifts, this assertion will fail.
#[test]
fn root_matches_canonical() {
    let sk: WinternitzPrivkey<N> = keypair().derive();
    let root = sk.to_pubkey().merklize();
    assert_eq!(
        root.as_bytes(),
        &[
            0x04, 0xe8, 0x1b, 0xec, 0x8e, 0xbc, 0xfd, 0x0c, 0x0a, 0x5b, 0x05, 0xb1, 0x37, 0xc7,
            0x77, 0x44, 0x89, 0xd9, 0x56, 0xbe, 0xee, 0xef, 0xea, 0x32, 0x6b, 0x8e, 0x1d, 0x6a,
            0x96, 0x9a, 0x78, 0x42,
        ]
    );
}

#[test]
fn sign_verify_roundtrip() {
    let sk: WinternitzPrivkey<N> = keypair().derive();
    let pk = sk.to_pubkey();
    let root = pk.merklize();

    let message: &[&[u8]] = &[b"hello winternitz".as_slice()];
    let sig: WinternitzSignature<N> = sk.sign(message);

    assert!(sig.verify(message, &root), "valid signature must verify");
}

#[test]
fn verify_rejects_modified_message() {
    let sk: WinternitzPrivkey<N> = keypair().derive();
    let root = sk.to_pubkey().merklize();
    let sig = sk.sign(&[b"original message".as_slice()]);

    assert!(!sig.verify(&[b"different message".as_slice()], &root));
}

#[test]
fn verify_rejects_wrong_root() {
    let kp_a = WinternitzKeypair::from_mnemonic(MNEMONIC, 0).unwrap();
    let kp_b = WinternitzKeypair::from_mnemonic(MNEMONIC, 1).unwrap();

    let root_b = kp_b.derive::<N>().to_pubkey().merklize();
    let sig = kp_a.derive::<N>().sign(&[b"message".as_slice()]);
    assert!(
        !sig.verify(&[b"message".as_slice()], &root_b),
        "sig from kp_a must not verify against root_b"
    );
}

#[test]
fn deterministic_signature() {
    // W-OTS+ is a deterministic function of (privkey, message_hash).
    let kp = keypair();
    let s1 = kp
        .derive::<N>()
        .sign(&[b"abc".as_slice()])
        .as_bytes()
        .to_vec();
    let s2 = kp
        .derive::<N>()
        .sign(&[b"abc".as_slice()])
        .as_bytes()
        .to_vec();
    assert_eq!(s1, s2);
}

#[test]
fn different_messages_produce_different_signatures() {
    let kp = keypair();
    let s1 = kp
        .derive::<N>()
        .sign(&[b"message one".as_slice()])
        .as_bytes()
        .to_vec();
    let s2 = kp
        .derive::<N>()
        .sign(&[b"message two".as_slice()])
        .as_bytes()
        .to_vec();
    assert_ne!(s1, s2);
}

#[test]
fn from_to_pubkey_roundtrip_via_merklize() {
    // Two ways to get a root from a privkey: explicit and via Into.
    let sk: WinternitzPrivkey<N> = keypair().derive();
    let pk_a = sk.to_pubkey();
    let pk_b: WinternitzPubkey<N> = (&sk).into();
    assert_eq!(pk_a.as_bytes(), pk_b.as_bytes());

    let root_a = pk_a.merklize();
    let root_b: WinternitzRoot = (&pk_b).into();
    assert_eq!(root_a, root_b);
}

#[test]
fn pubkey_tryfrom_bytes_zerocopy() {
    let sk: WinternitzPrivkey<N> = keypair().derive();
    let pk = sk.to_pubkey();
    let bytes = pk.as_bytes().to_vec();

    let pk_ref: &WinternitzPubkey<N> = bytes.as_slice().try_into().expect("correct length");
    assert_eq!(pk_ref.as_bytes(), pk.as_bytes());
    // Pointer should alias the source slice — true zero-copy.
    assert_eq!(pk_ref.as_bytes().as_ptr(), bytes.as_ptr());
}

#[test]
fn signature_tryfrom_bytes_zerocopy() {
    let kp = keypair();
    let root = kp.derive::<N>().to_pubkey().merklize();
    let sig = kp.derive::<N>().sign(&[b"x".as_slice()]);
    let bytes = sig.as_bytes().to_vec();

    let sig_ref: &WinternitzSignature<N> = bytes.as_slice().try_into().expect("correct length");
    assert_eq!(sig_ref.as_bytes(), sig.as_bytes());
    assert!(sig_ref.verify(&[b"x".as_slice()], &root));
}

#[test]
fn sign_and_increment_advances_position() {
    let mut kp = keypair();
    let root_0 = kp.derive::<N>().to_pubkey().merklize();

    let sig_0: WinternitzSignature<N> = kp.sign_and_increment(&[b"first".as_slice()]);
    assert!(sig_0.verify(&[b"first".as_slice()], &root_0));

    // After advancing, the keypair derives a different privkey/root.
    let root_1 = kp.derive::<N>().to_pubkey().merklize();
    assert_ne!(root_0, root_1);

    let sig_1: WinternitzSignature<N> = kp.sign_and_increment(&[b"second".as_slice()]);
    assert!(sig_1.verify(&[b"second".as_slice()], &root_1));
    assert!(
        !sig_1.verify(&[b"second".as_slice()], &root_0),
        "second sig must not verify against first root"
    );
}

#[test]
fn resume_at_position_matches_in_memory_advance() {
    // CLI flow: sign at position 0, persist position, restart at saved position.
    let mut kp_fresh = keypair();
    kp_fresh.sign_and_increment::<N>(&[b"first".as_slice()]);
    kp_fresh.sign_and_increment::<N>(&[b"second".as_slice()]);
    let (w, p, c) = (kp_fresh.wallet(), kp_fresh.parent(), kp_fresh.child());

    let kp_resumed = WinternitzKeypair::from_mnemonic_at(MNEMONIC, w, p, c).unwrap();
    assert_eq!(
        kp_fresh.derive::<N>().as_bytes(),
        kp_resumed.derive::<N>().as_bytes(),
    );
}

#[test]
fn tryfrom_rejects_wrong_length() {
    let too_short: &[u8] = &[0u8; 100];
    let r: Result<&WinternitzPubkey<N>, _> = too_short.try_into();
    assert!(r.is_err());

    let r: Result<&WinternitzSignature<N>, _> = too_short.try_into();
    assert!(r.is_err());

    let r: Result<&WinternitzPrivkey<N>, _> = too_short.try_into();
    assert!(r.is_err());
}
