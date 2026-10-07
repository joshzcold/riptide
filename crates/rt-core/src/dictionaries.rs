//! Spell-check dictionaries `:spell-install` can download: Chromium's own
//! `.bdic` files from its hunspell_dictionaries repository, at a pinned
//! commit, each checked against a pinned SHA-256 before it's saved.

use sha2::{Digest, Sha256};

/// The hunspell_dictionaries commit the files and hashes below come from.
pub const COMMIT: &str = "cee14e319bb7603a1157bb4d1e216be64ee82b77";

#[derive(Debug, PartialEq, Eq)]
pub struct Dictionary {
    /// As in `spellcheck.languages`, e.g. `en-US`.
    pub language: &'static str,
    /// The name Chromium looks for in the profile's `Dictionaries` folder.
    pub file: &'static str,
    pub sha256: &'static str,
    pub size: usize,
}

const fn d(
    language: &'static str,
    file: &'static str,
    sha256: &'static str,
    size: usize,
) -> Dictionary {
    Dictionary {
        language,
        file,
        sha256,
        size,
    }
}

/// The newest version of each language, the one Chromium loads.
pub const DICTIONARIES: &[Dictionary] = &[
    d(
        "af-ZA",
        "af-ZA-3-0.bdic",
        "b1cef1b6548dfefb9ba96e01e35db096b2c15e052ff7a32ca431f20b7819879e",
        2100027,
    ),
    d(
        "bg-BG",
        "bg-BG-3-0.bdic",
        "a25f46ea7b44951b23432b90a86f8bdd4b5eada215d765749213219b261017b5",
        1057506,
    ),
    d(
        "ca-ES",
        "ca-ES-3-0.bdic",
        "e03b0f6b4f1926e43eef08d6984915fe2615afe81c0fedb5c6b30671c0580504",
        1156964,
    ),
    d(
        "cs-CZ",
        "cs-CZ-3-0.bdic",
        "fd31ba12d939c43cc71d5a00441df9bace5edcdeca735ade0e84e5f10cd410d6",
        3108527,
    ),
    d(
        "cy-GB",
        "cy-GB-1-0.bdic",
        "58b4bbc81f418ff81926b4d323ea8c2aa100ae8905f151116e1dff68b22314f2",
        490804,
    ),
    d(
        "da-DK",
        "da-DK-3-0.bdic",
        "265ce85a15f2e470f7cf4bbefba0b457327af813e444f63c652f666e67de459f",
        970965,
    ),
    d(
        "de-DE",
        "de-DE-3-0.bdic",
        "076a470700dbd0aa62bb8b3d24c34e1340bdfe83cbbcfc035ce972f23c140400",
        6811094,
    ),
    d(
        "el-GR",
        "el-GR-3-0.bdic",
        "240777a5decb51a1d22756401d8c0155e094d39ba92360aafaaff73a1cbd328c",
        6452802,
    ),
    d(
        "en-AU",
        "en-AU-10-2.bdic",
        "13427b9828773291cde00ba266a58fd5110ba6f1d4b29ed86b49f4b3afdf0a39",
        453801,
    ),
    d(
        "en-CA",
        "en-CA-10-2.bdic",
        "4e0ab8e315fff2c7db23476e08a287088005912a08d9d5a2efc601699b7d34a8",
        451827,
    ),
    d(
        "en-GB",
        "en-GB-10-2.bdic",
        "e014c23d2215f093205343b9b8805e17c8d8e115d314d93fd5ba51eadf05cff2",
        452008,
    ),
    d(
        "en-GB-oxendict",
        "en-GB-oxendict-10-2.bdic",
        "e01c756792d6e7a0bd62bfa5c0264d42971b479865aecc204463146909300e1c",
        451953,
    ),
    d(
        "en-US",
        "en-US-10-2.bdic",
        "b0d80e6b6c5491879365e176ecbd98074602a663429bcfcfb0a5a8375c051f13",
        451982,
    ),
    d(
        "es-ES",
        "es-ES-3-0.bdic",
        "e6334dcf080aaeca679db70565762a2c296ff5780c1af263530ac7345736bfa9",
        785066,
    ),
    d(
        "et-EE",
        "et-EE-3-0.bdic",
        "282c6ba4a34740dd06bb37de88b7950c8979096bf62cd4e648f1121c86b1d428",
        2458725,
    ),
    d(
        "fa-IR",
        "fa-IR-9-0.bdic",
        "8c0084622b9ec19f9bb4a024e3df497ecc615a33a095f2e0a5db9eb7a1655031",
        1044412,
    ),
    d(
        "fo-FO",
        "fo-FO-3-0.bdic",
        "4118aa699106e2a725af1464aaa72f066a8890f3d452ed3c8a9f755e49afb7d4",
        1537517,
    ),
    d(
        "fr-FR",
        "fr-FR-3-0.bdic",
        "6cffc13b549c33d44180a3a7b13943fe0eadfbe80bcdbccc76f67c55c2012968",
        1074744,
    ),
    d(
        "gl",
        "gl-1-0.bdic",
        "15414fc4ff97e7119dbcfe536b5d509fd99f6ffa7ee43e988d9db8ca8f64c805",
        2252121,
    ),
    d(
        "he-IL",
        "he-IL-3-0.bdic",
        "66e9d835e9f40601a46ed5fe1a671a22f7c3440b39d6cb154648681230b7f638",
        2272411,
    ),
    d(
        "hi-IN",
        "hi-IN-3-0.bdic",
        "7b1eb10f38f57a4638fdb1a9f08fe0765b5791c23ebeec6f5f28de32e4886f9b",
        223868,
    ),
    d(
        "hr-HR",
        "hr-HR-3-0.bdic",
        "2c412f41b48c80ab8bb04705c6aece9ca0d39f4a5cb1b5098708ca29477c9b60",
        1588645,
    ),
    d(
        "hu-HU",
        "hu-HU-3-0.bdic",
        "9d4462fa6a49f742b02ce0291033b7df71ccfbe6b23dabaa5513d3b66dbd3be9",
        2784888,
    ),
    d(
        "hy",
        "hy-1-0.bdic",
        "f6d306587c69cdb1231f44300b0426cc96c5262ee29d7e490af6623f3e56d585",
        2191121,
    ),
    d(
        "id-ID",
        "id-ID-3-0.bdic",
        "3726e15a96fcba6d54302386de742c011af68b2de491f099bbf917fc23fd74af",
        231105,
    ),
    d(
        "it-IT",
        "it-IT-3-0.bdic",
        "8b83a0d4bba4cfcd4a4e7ee89b703014a00a5c08e721e568a5c552a732827fff",
        983219,
    ),
    d(
        "ko",
        "ko-3-0.bdic",
        "75b5e033a0d6e1bba1e7ce6aad6f723d84128cda7f2048339c8c498f049f49bc",
        11476456,
    ),
    d(
        "lt-LT",
        "lt-LT-3-0.bdic",
        "ab581380a130cfc8601556264370510e104a08ac4326daa132e93528e6784710",
        917849,
    ),
    d(
        "lv-LV",
        "lv-LV-3-0.bdic",
        "6229b4090600e584d06f826b7efb55f338e14cb653779386d4b6feb4d9716667",
        1828731,
    ),
    d(
        "nb-NO",
        "nb-NO-3-0.bdic",
        "14b5601cbccb6b270ccc421cb687dc490ecb3ad96ffc2bb8bc4ebee5d379168f",
        1534513,
    ),
    d(
        "nl-NL",
        "nl-NL-3-0.bdic",
        "43eea1bd8149c1a24f4388ade6a261a981ca2e786e7df12e606295a8024b150e",
        1807935,
    ),
    d(
        "pl-PL",
        "pl-PL-3-0.bdic",
        "a04c51759b6e54f01e628dfc6354ca88a0711d8bcefa6a8c0e8baa04156fb92e",
        3233589,
    ),
    d(
        "pt-BR",
        "pt-BR-3-0.bdic",
        "6b2850f5a54994a5204a9a88d4b586e9d4e028a0360b67352b04cffdb2a3e0ea",
        3923495,
    ),
    d(
        "pt-PT",
        "pt-PT-3-0.bdic",
        "a6dfc3332b15a6cdf06be3f28024f22e40e0467cf0400f8ce9c43cf1af2c3f40",
        407258,
    ),
    d(
        "ro-RO",
        "ro-RO-3-0.bdic",
        "0db54e3f28291d0fd28f049a61a76ade5a98591b6ae16cb7113e2561b0760ec4",
        1478687,
    ),
    d(
        "ru-RU",
        "ru-RU-3-0.bdic",
        "92dcc64fb0b0c065b443abf9de6fb58116b2a64c49427d3715fa7536ad1c53b8",
        2309618,
    ),
    d(
        "sh",
        "sh-4-0.bdic",
        "7d2fb58d3e5791e047d97d85d64714eda583351fcfc9dbe076335af77dd6111c",
        2755632,
    ),
    d(
        "sk-SK",
        "sk-SK-3-0.bdic",
        "c4ef7e950bda27505c9103cd72abc01cd8d3f1a36d85708b9cc6d8e7dd230547",
        1668425,
    ),
    d(
        "sl-SI",
        "sl-SI-3-0.bdic",
        "92427a12b6796b043bfb99d39b12ed09b0210ab5ab7dcad31e5b2158a7da17d8",
        2020525,
    ),
    d(
        "sq",
        "sq-3-0.bdic",
        "b4498e4ccc005fa492d7c9bb35b7c88fa307952342333790bba8f16a4a8b5d7f",
        1645240,
    ),
    d(
        "sr",
        "sr-4-0.bdic",
        "ec2660e14e2799fdcbbe897bde440d382869623efe3e63f844105c4e99921a17",
        3682587,
    ),
    d(
        "sv-SE",
        "sv-SE-3-0.bdic",
        "1823f1ae5ed2b1568924894d6447c2efa5bd5510cd71cd4bd2e9e251ff367447",
        1487321,
    ),
    d(
        "ta-IN",
        "ta-IN-3-0.bdic",
        "fa354a4e6b27704b9efeee71ed29b6956f93fb5c6f9bd64df39a4e72fb54c9ce",
        1410211,
    ),
    d(
        "tg-TG",
        "tg-TG-5-0.bdic",
        "d474ac5e044409d54b394b4b8ffb47e7036884d2d1e22e2832e887f3006e75c9",
        1173108,
    ),
    d(
        "tr-TR",
        "tr-TR-4-0.bdic",
        "3085511d86a0a3bc65d970ae98c4f89a48d5405148297a5d48fab3c8c85e7f70",
        9644083,
    ),
    d(
        "uk-UA",
        "uk-UA-5-0.bdic",
        "7c2b48c7bbc6f70bd4d74cb050ee7105848506361f5078b4e574bea3c6089e54",
        5327186,
    ),
    d(
        "vi-VN",
        "vi-VN-3-0.bdic",
        "451ab9d873a39a38a6cabd52f35b4787837a63473d712023c73e59cf9e4a4aca",
        60036,
    ),
];

/// The dictionary for `language` (`en-US`, `en_us` or `EN-us`).
pub fn find(language: &str) -> Option<&'static Dictionary> {
    let wanted = language.trim().replace('_', "-");
    DICTIONARIES
        .iter()
        .find(|d| d.language.eq_ignore_ascii_case(&wanted))
}

impl Dictionary {
    /// The language's English name, e.g. `German` for `de-DE`.
    pub fn name(&self) -> &'static str {
        match self.language {
            "af-ZA" => "Afrikaans",
            "bg-BG" => "Bulgarian",
            "ca-ES" => "Catalan",
            "cs-CZ" => "Czech",
            "cy-GB" => "Welsh",
            "da-DK" => "Danish",
            "de-DE" => "German",
            "el-GR" => "Greek",
            "en-AU" => "English (Australia)",
            "en-CA" => "English (Canada)",
            "en-GB" => "English (UK)",
            "en-GB-oxendict" => "English (UK, Oxford spelling)",
            "en-US" => "English (US)",
            "es-ES" => "Spanish",
            "et-EE" => "Estonian",
            "fa-IR" => "Persian",
            "fo-FO" => "Faroese",
            "fr-FR" => "French",
            "gl" => "Galician",
            "he-IL" => "Hebrew",
            "hi-IN" => "Hindi",
            "hr-HR" => "Croatian",
            "hu-HU" => "Hungarian",
            "hy" => "Armenian",
            "id-ID" => "Indonesian",
            "it-IT" => "Italian",
            "ko" => "Korean",
            "lt-LT" => "Lithuanian",
            "lv-LV" => "Latvian",
            "nb-NO" => "Norwegian (Bokmål)",
            "nl-NL" => "Dutch",
            "pl-PL" => "Polish",
            "pt-BR" => "Portuguese (Brazil)",
            "pt-PT" => "Portuguese (Portugal)",
            "ro-RO" => "Romanian",
            "ru-RU" => "Russian",
            "sh" => "Serbo-Croatian",
            "sk-SK" => "Slovak",
            "sl-SI" => "Slovenian",
            "sq" => "Albanian",
            "sr" => "Serbian",
            "sv-SE" => "Swedish",
            "ta-IN" => "Tamil",
            "tg-TG" => "Tajik",
            "tr-TR" => "Turkish",
            "uk-UA" => "Ukrainian",
            "vi-VN" => "Vietnamese",
            _ => "",
        }
    }

    /// Where to download it: base64 text, as gitiles serves files.
    pub fn url(&self) -> String {
        format!(
            "https://chromium.googlesource.com/chromium/deps/hunspell_dictionaries/+/{COMMIT}/{}?format=TEXT",
            self.file
        )
    }

    /// Whether `bytes` is this dictionary, byte for byte.
    pub fn verify(&self, bytes: &[u8]) -> Result<(), String> {
        if bytes.len() != self.size {
            return Err(format!(
                "{} is {} bytes, not the expected {}",
                self.file,
                bytes.len(),
                self.size
            ));
        }
        let digest = Sha256::digest(bytes);
        let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
        if hex != self.sha256 {
            return Err(format!("{} doesn't match its pinned checksum", self.file));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_are_found_however_they_are_written() {
        assert_eq!(find("en-US").map(|d| d.file), Some("en-US-10-2.bdic"));
        assert_eq!(find("en_us").map(|d| d.file), Some("en-US-10-2.bdic"));
        assert_eq!(find("uk-UA").map(|d| d.file), Some("uk-UA-5-0.bdic"));
        assert!(find("xx-XX").is_none());
        assert!(find("klingon").is_none());
    }

    #[test]
    fn every_entry_is_one_language_with_a_full_hash() {
        let mut seen = std::collections::BTreeSet::new();
        for d in DICTIONARIES {
            assert!(!d.name().is_empty(), "{} has no name", d.language);
            assert!(seen.insert(d.language), "{} twice", d.language);
            assert!(d.file.starts_with(d.language) && d.file.ends_with(".bdic"));
            assert_eq!(d.sha256.len(), 64);
            assert!(d.size > 0);
        }
    }

    #[test]
    fn verify_checks_size_and_hash() {
        let fake = Dictionary {
            language: "zz",
            file: "zz-1-0.bdic",
            // SHA-256 of "abc".
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            size: 3,
        };
        assert!(fake.verify(b"abc").is_ok());
        assert!(fake.verify(b"abx").unwrap_err().contains("checksum"));
        assert!(fake.verify(b"ab").unwrap_err().contains("bytes"));
    }
}
