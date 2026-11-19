//! sign-update (F-34): signs Fermata's update files for a release.
//!
//!   sign-update keygen <dir>          writes <dir>/fermata-update.key (secret, keep it outside the repository) and
//!                                     <dir>/fermata-update.pub (copy it to assets/update/fermata-update.pub)
//!   sign-update sign <key> <file>     writes <file>.sig beside the file
//!   sign-update verify <pub> <file>   checks <file> against <file>.sig
#[path = "../../../src/update_sig.rs"]
mod update_sig;

use std::path::Path;

fn fail(msg: &str) -> ! {
    eprintln!("sign-update: {msg}");
    std::process::exit(1)
}

fn read_secret(path: &str) -> [u8; 32] {
    let t = std::fs::read_to_string(path).unwrap_or_else(|e| fail(&format!("can't read {path}: {e}")));
    update_sig::unhex(&t).and_then(|v| v.try_into().ok()).unwrap_or_else(|| fail("the key file isn't a Fermata update key"))
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.iter().map(|s| s.as_str()).collect::<Vec<_>>().as_slice() {
        ["keygen", dir] => {
            let dir = Path::new(dir);
            let key = dir.join("fermata-update.key");
            if key.exists() {
                fail(&format!("{} already exists; not overwriting it", key.display()));
            }
            std::fs::create_dir_all(dir).unwrap_or_else(|e| fail(&e.to_string()));
            let mut secret = [0u8; 32];
            getrandom::fill(&mut secret).unwrap_or_else(|e| fail(&format!("no random numbers: {e}")));
            std::fs::write(&key, format!("{}\n", update_sig::hex(&secret))).unwrap_or_else(|e| fail(&e.to_string()));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600));
            }
            let public = update_sig::public_of(&secret);
            std::fs::write(dir.join("fermata-update.pub"), format!("{public}\n")).unwrap_or_else(|e| fail(&e.to_string()));
            println!("secret key: {} (keep it outside the repository)", key.display());
            println!("public key: {public}");
            println!("copy {} to assets/update/fermata-update.pub", dir.join("fermata-update.pub").display());
        }
        ["sign", key, file] => {
            let secret = read_secret(key);
            let data = std::fs::read(file).unwrap_or_else(|e| fail(&format!("can't read {file}: {e}")));
            let name = Path::new(file).file_name().and_then(|n| n.to_str()).unwrap_or_else(|| fail("bad file name"));
            let sig = update_sig::sign(&ed25519_dalek::SigningKey::from_bytes(&secret), name, &data);
            let out = format!("{file}.sig");
            std::fs::write(&out, &sig).unwrap_or_else(|e| fail(&e.to_string()));
            println!("signed {name} ({} bytes) -> {out}", data.len());
        }
        ["verify", public, file] => {
            let public = std::fs::read_to_string(public).unwrap_or_else(|e| fail(&e.to_string()));
            let data = std::fs::read(file).unwrap_or_else(|e| fail(&format!("can't read {file}: {e}")));
            let sig = std::fs::read_to_string(format!("{file}.sig")).unwrap_or_else(|e| fail(&e.to_string()));
            let name = Path::new(file).file_name().and_then(|n| n.to_str()).unwrap_or("");
            match update_sig::verify(public.trim(), name, &data, &sig) {
                Ok(()) => println!("{name}: signature OK"),
                Err(e) => fail(&format!("{name}: {e}")),
            }
        }
        _ => {
            eprintln!("usage:\n  sign-update keygen <dir>\n  sign-update sign <key> <file>\n  sign-update verify <pub> <file>");
            std::process::exit(2);
        }
    }
}
