//! Exact upstream voice artifacts for the existing nine runtime profiles.
//! Availability and metadata compatibility do not claim observed game/audio success.
use super::NativeVoiceProfile;
use crate::engine::connect_ready_profile::ReadyProfile;

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub(crate) struct VoiceArtifact {
    pub relative: &'static str,
    pub bytes: u64,
    pub sha256: &'static str,
    pub url: &'static str,
}

const A0: VoiceArtifact = VoiceArtifact {
    relative: "mods/voicechat-forge-1.12.2-2.6.22.jar",
    bytes: 5875010,
    sha256: "91557cd460a8b6f3c57e2536e6c752a9c13fb685fcf208ec1e90aa65d31a714d",
    url:
        "https://cdn.modrinth.com/data/9eGKb6K1/versions/8FwSEDPE/voicechat-forge-1.12.2-2.6.22.jar",
};
const A1: VoiceArtifact = VoiceArtifact {
    relative: "mods/voicechat-fabric-1.16.5-2.6.22.jar",
    bytes: 5138361,
    sha256: "21db372d93318631bfbb1220bf81e6d80304cd97f1cf2be02895da601292d129",
    url: "https://cdn.modrinth.com/data/9eGKb6K1/versions/4cPSgvFX/voicechat-fabric-1.16.5-2.6.22.jar",
};
const A2: VoiceArtifact = VoiceArtifact {
    relative: "mods/voicechat-forge-1.16.5-2.6.22.jar",
    bytes: 4921098,
    sha256: "2573d59cdf7e2603d45b46e5339be66f474ecd1c6facf4969683a83a2830b9ed",
    url:
        "https://cdn.modrinth.com/data/9eGKb6K1/versions/oYfyjdA6/voicechat-forge-1.16.5-2.6.22.jar",
};
const A3: VoiceArtifact = VoiceArtifact {
    relative: "mods/voicechat-fabric-1.19.2-2.6.22.jar",
    bytes: 5227844,
    sha256: "be11fda1858247b91b093ad6a8ab9ec3eb7e457861fc61c88b290ec8af45fb6e",
    url: "https://cdn.modrinth.com/data/9eGKb6K1/versions/Ij93aQEk/voicechat-fabric-1.19.2-2.6.22.jar",
};
const A4: VoiceArtifact = VoiceArtifact {
    relative: "mods/voicechat-forge-1.19.2-2.6.22.jar",
    bytes: 5362155,
    sha256: "5c6fa7e1a84487835edaeed5823083704172fdb1196dacf15761a5d60bd6d51b",
    url:
        "https://cdn.modrinth.com/data/9eGKb6K1/versions/uLGZpAgJ/voicechat-forge-1.19.2-2.6.22.jar",
};
const A5: VoiceArtifact = VoiceArtifact {
    relative: "mods/voicechat-fabric-1.20.1-2.6.22.jar",
    bytes: 5279173,
    sha256: "6a595ad90c3ab2cc9dfd25443aa404a7d18e91f84658d67123642de959c7f7a9",
    url: "https://cdn.modrinth.com/data/9eGKb6K1/versions/u5T0ji6n/voicechat-fabric-1.20.1-2.6.22.jar",
};
const A6: VoiceArtifact = VoiceArtifact {
    relative: "mods/voicechat-forge-1.20.1-2.6.22.jar",
    bytes: 4923439,
    sha256: "727d31c11905be6af05686c619dd62a1ed4e451c47670cf5e9804c1ac8871386",
    url:
        "https://cdn.modrinth.com/data/9eGKb6K1/versions/S11m0QIb/voicechat-forge-1.20.1-2.6.22.jar",
};
const A7: VoiceArtifact = VoiceArtifact {
    relative: "mods/voicechat-fabric-1.21.1-2.6.22.jar",
    bytes: 5369475,
    sha256: "972a01592f670aa57f2dc2e3587a656b798106037468c61e4bfb49eba7d6ae36",
    url: "https://cdn.modrinth.com/data/9eGKb6K1/versions/VS5X7jZd/voicechat-fabric-1.21.1-2.6.22.jar",
};
const A8: VoiceArtifact = VoiceArtifact {
    relative: "mods/voicechat-forge-1.21.1-2.6.22.jar",
    bytes: 4914752,
    sha256: "a96fb65f053293e49ecd24a8679ee6e9be5b01dfee207c871997acf2bfdfd368",
    url:
        "https://cdn.modrinth.com/data/9eGKb6K1/versions/4LuAeiNl/voicechat-forge-1.21.1-2.6.22.jar",
};
const A9: VoiceArtifact = VoiceArtifact {
    relative: "mods/plasmovoice-fabric-1.16.5-2.1.17.jar",
    bytes: 12335242,
    sha256: "97b956a449a908118294ca16a600f1e5b49acd4a3f7cf939f20e347726e6ee8b",
    url: "https://cdn.modrinth.com/data/1bZhdhsH/versions/gEu4XS1L/plasmovoice-fabric-1.16.5-2.1.17.jar",
};
const A10: VoiceArtifact = VoiceArtifact {
    relative: "mods/plasmovoice-forge-1.16.5-2.1.17.jar",
    bytes: 12319461,
    sha256: "521e780666d3db21dd9a6afd9ea1382618a68a229c8209519921251c6b62641c",
    url: "https://cdn.modrinth.com/data/1bZhdhsH/versions/sVbz4VTK/plasmovoice-forge-1.16.5-2.1.17.jar",
};
const A11: VoiceArtifact = VoiceArtifact {
    relative: "mods/plasmovoice-fabric-1.19.2-2.1.17.jar",
    bytes: 12352072,
    sha256: "0cdc958ca899a7097d159f8ee16d46182f83beb05c2688ac371d0655ac12e366",
    url: "https://cdn.modrinth.com/data/1bZhdhsH/versions/MRO4EUl3/plasmovoice-fabric-1.19.2-2.1.17.jar",
};
const A12: VoiceArtifact = VoiceArtifact {
    relative: "mods/plasmovoice-forge-1.19.2-2.1.17.jar",
    bytes: 12335111,
    sha256: "cbc2a4e40ce7ea0ff2822e0233d8276d19b43e3838211a683d72a2d47407e8c4",
    url: "https://cdn.modrinth.com/data/1bZhdhsH/versions/F1VKyzib/plasmovoice-forge-1.19.2-2.1.17.jar",
};
const A13: VoiceArtifact = VoiceArtifact {
    relative: "mods/plasmovoice-fabric-1.20.1-2.1.17.jar",
    bytes: 12354369,
    sha256: "3c999d235e1c58001a3e8fd8ba46dde6b0d984f3e662806760153f294d9f4d3f",
    url: "https://cdn.modrinth.com/data/1bZhdhsH/versions/rFDDAPes/plasmovoice-fabric-1.20.1-2.1.17.jar",
};
const A14: VoiceArtifact = VoiceArtifact {
    relative: "mods/plasmovoice-forge-1.20.1-2.1.17.jar",
    bytes: 12337455,
    sha256: "2f2ae3a5856b44b460e72030a95318fc3657ae4c24ee3f3e16bcb5cbaa42e20d",
    url: "https://cdn.modrinth.com/data/1bZhdhsH/versions/KoHwlbDf/plasmovoice-forge-1.20.1-2.1.17.jar",
};
const A15: VoiceArtifact = VoiceArtifact {
    relative: "mods/plasmovoice-fabric-1.21.1-2.1.17.jar",
    bytes: 12365966,
    sha256: "46e1a4ae51786457712991ba05bf573e2d11331aeadb63b7d108839def022a2b",
    url: "https://cdn.modrinth.com/data/1bZhdhsH/versions/6Q2QmcUp/plasmovoice-fabric-1.21.1-2.1.17.jar",
};
const A16: VoiceArtifact = VoiceArtifact {
    relative: "mods/plasmovoice-forge-1.21.1-2.1.17.jar",
    bytes: 12341307,
    sha256: "cf66167c874bcd756b63d3cae3a35656eed206f061ac36cf90aa3d5492a05cf2",
    url: "https://cdn.modrinth.com/data/1bZhdhsH/versions/lkNAkqrJ/plasmovoice-forge-1.21.1-2.1.17.jar",
};
const A17: VoiceArtifact = VoiceArtifact {
    relative: "mods/fabric-api-0.42.0+1.16.jar",
    bytes: 1090414,
    sha256: "3df8dd503f35aa0ac9fab8ad9f9a369fdfd0b1ab544af19a3d626d948fb4586c",
    url:
        "https://cdn.modrinth.com/data/P7dR8mSH/versions/0.42.0%2B1.16/fabric-api-0.42.0%2B1.16.jar",
};
const A18: VoiceArtifact = VoiceArtifact {
    relative: "mods/fabric-api-0.77.0+1.19.2.jar",
    bytes: 1892953,
    sha256: "0fc62744a38f629fe0840893713e6a111ed1263719f93e499c990688811824ec",
    url: "https://cdn.modrinth.com/data/P7dR8mSH/versions/6g95K303/fabric-api-0.77.0%2B1.19.2.jar",
};
const A19: VoiceArtifact = VoiceArtifact {
    relative: "mods/fabric-api-0.92.12+1.20.1.jar",
    bytes: 2137232,
    sha256: "4197ff4fbdac13cffccd267c1bc59e9fbabb2b5683a9d5f8023f4b5ea16a1c1e",
    url: "https://cdn.modrinth.com/data/P7dR8mSH/versions/rvI2dfzR/fabric-api-0.92.12%2B1.20.1.jar",
};
const A20: VoiceArtifact = VoiceArtifact {
    relative: "mods/fabric-api-0.116.10+1.21.1.jar",
    bytes: 2425988,
    sha256: "af0eb792940568fdb1f37bb65a2599773bc47d2010aedf5ec5ed0bd3b6fc40e6",
    url:
        "https://cdn.modrinth.com/data/P7dR8mSH/versions/ID8pq1x1/fabric-api-0.116.10%2B1.21.1.jar",
};

/// None is intentional for Plasmo on Forge 1.12.2: no upstream artifact exists.
pub(crate) fn artifacts(
    runtime: ReadyProfile,
    voice: NativeVoiceProfile,
) -> Option<&'static [VoiceArtifact]> {
    use NativeVoiceProfile::{PlasmoVoice as Plasmo, SimpleVoiceChat as Svc};
    use ReadyProfile::*;
    match (runtime, voice) {
        (Forge1122, Svc) => Some(&[A0]),
        (Forge1122, Plasmo) => None,
        (Fabric1165, Svc) => Some(&[A1]),
        (Fabric1165, Plasmo) => Some(&[A9, A17]),
        (Forge1165, Svc) => Some(&[A2]),
        (Forge1165, Plasmo) => Some(&[A10]),
        (Fabric1192, Svc) => Some(&[A3]),
        (Fabric1192, Plasmo) => Some(&[A11, A18]),
        (Forge1192, Svc) => Some(&[A4]),
        (Forge1192, Plasmo) => Some(&[A12]),
        (Fabric1201, Svc) => Some(&[A5]),
        (Fabric1201, Plasmo) => Some(&[A13, A19]),
        (Forge1201, Svc) => Some(&[A6]),
        (Forge1201, Plasmo) => Some(&[A14]),
        (Fabric1211, Svc) => Some(&[A7]),
        (Fabric1211, Plasmo) => Some(&[A15, A20]),
        (Forge1211, Svc) => Some(&[A8]),
        (Forge1211, Plasmo) => Some(&[A16]),
    }
}
