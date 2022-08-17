use fvm_sdk as sdk;
use fvm_shared::crypto::hash::SupportedHashes;
use fvm_shared::error::ExitCode;

include!(concat!(env!("OUT_DIR"), "/wasm_binary.rs"));

#[no_mangle]
pub fn invoke(_: u32) -> u32 {
    std::panic::set_hook(Box::new(|info| {
        sdk::vm::abort(
            ExitCode::USR_ASSERTION_FAILED.value(),
            Some(&format!("{}", info)),
        )
    }));

    test_expected_hash();
    test_hash_syscall();

    #[cfg(coverage)]
    sdk::debug::store_artifact("syscall_actor.profraw", minicov::capture_coverage());
    0
}

// use SDK methods to hash and compares against locally (inside the actor) hashed digest
fn test_expected_hash() {
    use multihash::MultihashDigest;
    let test_bytes = b"foo bar baz boxy";

    let blake_local = SupportedHashes::Blake2b256.digest(test_bytes);
    let blake_arr = sdk::crypto::hash_blake2b(test_bytes); // test against old SDK method since it does less unsafe things
    let blake_vec = sdk::crypto::hash(SupportedHashes::Blake2b256, test_bytes);

    assert_eq!(blake_arr.as_slice(), blake_vec.as_slice());
    assert_eq!(blake_local.digest(), blake_vec.as_slice());

    // macros dont work so im stuck with writing this out manually

    //sha
    {
        let local_digest = SupportedHashes::Sha2_256.digest(test_bytes);
        let digest = sdk::crypto::hash(SupportedHashes::Sha2_256, test_bytes);

        assert_eq!(local_digest.digest(), digest.as_slice());
    }
    // keccack
    {
        let local_digest = SupportedHashes::Keccak256.digest(test_bytes);
        let digest = sdk::crypto::hash(SupportedHashes::Keccak256, test_bytes);

        assert_eq!(local_digest.digest(), digest.as_slice());
    }
    // ripemd
    {
        let local_digest = SupportedHashes::Ripemd160.digest(test_bytes);
        let digest = sdk::crypto::hash(SupportedHashes::Ripemd160, test_bytes);

        assert_eq!(local_digest.digest(), digest.as_slice());
    }
}

// do funky things with hash syscall directly
fn test_hash_syscall() {
    use fvm_shared::error::ErrorNumber;
    use sdk::sys::crypto;

    let test_bytes = b"the quick fox jumped over the lazy dog";
    let mut buffer = [0u8; 64];

    let hasher: u64 = SupportedHashes::Sha2_256.into();
    let known_digest = sdk::crypto::hash(SupportedHashes::Sha2_256, test_bytes);

    // normal case
    unsafe {
        let written = crypto::hash(
            hasher,
            test_bytes.as_ptr(),
            test_bytes.len() as u32,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
        .unwrap_or_else(|_| panic!("failed compute hash using {:?}", hasher));
        assert_eq!(&buffer[..written as usize], known_digest.as_slice())
    }
    // invalid hash code
    unsafe {
        let e = crypto::hash(
            0xFF,
            test_bytes.as_ptr(),
            test_bytes.len() as u32,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
        .expect_err("Expected err from invalid code, got written bytes");
        assert_eq!(e, ErrorNumber::IllegalArgument)
    }
    // data pointer OOB
    unsafe {
        let e = crypto::hash(
            hasher,
            (u32::MAX) as *const u8, // pointer OOB
            test_bytes.len() as u32,
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
        .expect_err("Expected err, got written bytes");
        assert_eq!(e, ErrorNumber::IllegalArgument)
    }
    // data length OOB
    unsafe {
        let e = crypto::hash(
            hasher,
            test_bytes.as_ptr(),
            (u32::MAX / 2) as u32, // byte length OOB (2GB)
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
        .expect_err("Expected err, got written bytes");
        assert_eq!(e, ErrorNumber::IllegalArgument)
    }
    // digest buffer pointer OOB
    unsafe {
        let e = crypto::hash(
            hasher,
            test_bytes.as_ptr(),
            test_bytes.len() as u32,
            (u32::MAX) as *mut u8, // pointer OOB
            buffer.len() as u32,
        )
        .expect_err("Expected err, got written bytes");
        assert_eq!(e, ErrorNumber::IllegalArgument)
    }
    // digest length out of memory
    unsafe {
        let e = crypto::hash(
            hasher,
            test_bytes.as_ptr(),
            test_bytes.len() as u32,
            buffer.as_mut_ptr(),
            (u32::MAX / 2) as u32, // byte length OOB (2GB)
        )
        .expect_err("Expected err, got written bytes");
        assert_eq!(e, ErrorNumber::IllegalArgument)
    }
    // write bytes to the same buffer read from. (overlapping buffers is OK)
    unsafe {
        let len = test_bytes.len();
        // fill with "garbage"
        buffer.fill(0x69);
        buffer[..len].copy_from_slice(test_bytes);

        let written = crypto::hash(
            hasher,
            // read from buffer...
            buffer.as_ptr(),
            len as u32,
            // and write to the same one
            buffer.as_mut_ptr(),
            buffer.len() as u32,
        )
        .expect("Overlapping buffers should be allowed");
        assert_eq!(&buffer[..written as usize], known_digest.as_slice())
    }
}
