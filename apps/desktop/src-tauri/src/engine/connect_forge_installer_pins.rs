//! Forge installer pins shared by the reviewed Connect client and server.
use super::{connect_ready_profile::ReadyProfile, recovery};
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};

#[derive(Clone, Copy)]
pub(crate) struct ForgeInstallerPin {
    pub(crate) filename: &'static str,
    pub(crate) bytes: u64,
    pub(crate) sha256: &'static str,
    pub(crate) url: &'static str,
}

impl ForgeInstallerPin {
    pub(crate) fn for_runtime(runtime: ReadyProfile) -> Option<Self> {
        Some(match runtime {
            ReadyProfile::Forge1122 => Self {
                filename: "forge-1.12.2-14.23.5.2860-installer.jar",
                bytes: 4_681_950,
                sha256: "ea7c33ba95e3993a98d0e9e38168c0759ec323a18675a71d938e1f3f70e6e8e7",
                url: "https://maven.minecraftforge.net/net/minecraftforge/forge/1.12.2-14.23.5.2860/forge-1.12.2-14.23.5.2860-installer.jar",
            },
            ReadyProfile::Forge1165 => Self {
                filename: "forge-1.16.5-36.2.42-installer.jar",
                bytes: 7_543_237,
                sha256: "dfa90c681f30be2406548b6429bccb68d95af4f64ab19b8c58f6128fd50ff857",
                url: "https://maven.minecraftforge.net/net/minecraftforge/forge/1.16.5-36.2.42/forge-1.16.5-36.2.42-installer.jar",
            },
            ReadyProfile::Forge1192 => Self {
                filename: "forge-1.19.2-43.4.16-installer.jar",
                bytes: 7_169_133,
                sha256: "3f31e31b9518348279ff00bd97309b28120c036168c4347d76308013b308d56e",
                url: "https://maven.minecraftforge.net/net/minecraftforge/forge/1.19.2-43.4.16/forge-1.19.2-43.4.16-installer.jar",
            },
            ReadyProfile::Forge1201 => Self {
                filename: "forge-1.20.1-47.4.23-installer.jar",
                bytes: 8_836_161,
                sha256: "75cfcb11f60cc641dc83757c1357c08c9bdba255ec529feb115c5e89a59cf75a",
                url: "https://maven.minecraftforge.net/net/minecraftforge/forge/1.20.1-47.4.23/forge-1.20.1-47.4.23-installer.jar",
            },
            ReadyProfile::Forge1211 => Self {
                filename: "forge-1.21.1-52.1.12-installer.jar",
                bytes: 9_028_394,
                sha256: "6eb45efffa51f8f1e3a0c5876978f186a3dfc7db8df8a5833d9b645e774f777d",
                url: "https://maven.minecraftforge.net/net/minecraftforge/forge/1.21.1-52.1.12/forge-1.21.1-52.1.12-installer.jar",
            },
            _ => return None,
        })
    }

    pub(crate) fn pin_file(
        self,
        path: &Path,
    ) -> Result<recovery::SecureFile, recovery::RecoveryError> {
        if path.file_name().and_then(|name| name.to_str()) != Some(self.filename) {
            return Err(recovery::RecoveryError::Integrity);
        }
        let mut file = recovery::secure_open(path)?;
        if file.metadata()?.len() != self.bytes {
            return Err(recovery::RecoveryError::Integrity);
        }
        let mut hash = Sha256::new();
        let mut total = 0;
        let mut buffer = [0; 64 * 1024];
        loop {
            let read = file.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            total += read as u64;
            if total > self.bytes {
                return Err(recovery::RecoveryError::Integrity);
            }
            hash.update(&buffer[..read]);
        }
        if total != self.bytes || format!("{:x}", hash.finalize()) != self.sha256 {
            return Err(recovery::RecoveryError::Integrity);
        }
        Ok(file)
    }
}
