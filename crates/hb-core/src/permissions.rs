//! Which site permissions are granted, from `content.*` settings and the
//! session's remembered answers. Bit values match CEF's permission types.

use std::collections::HashMap;

use crate::settings::Settings;

pub struct Feature {
    pub bit: u32,
    pub label: &'static str,
    /// `ask`/`true`/`false` setting; features without one always ask.
    pub setting: Option<&'static str>,
}

/// `cef_permission_request_types_t` (permission prompts).
pub const PROMPT_FEATURES: &[Feature] = &[
    Feature {
        bit: 4,
        label: "use your camera",
        setting: Some("content.media.video_capture"),
    },
    Feature {
        bit: 4096,
        label: "use your microphone",
        setting: Some("content.media.audio_capture"),
    },
    Feature {
        bit: 256,
        label: "know your location",
        setting: Some("content.geolocation"),
    },
    Feature {
        bit: 32768,
        label: "show notifications",
        setting: Some("content.notifications.enabled"),
    },
    Feature {
        bit: 16,
        label: "read your clipboard",
        setting: None,
    },
    Feature {
        bit: 2,
        label: "pan, tilt and zoom your camera",
        setting: None,
    },
    Feature {
        bit: 128,
        label: "use your local fonts",
        setting: None,
    },
    Feature {
        bit: 2048,
        label: "know when you're idle",
        setting: None,
    },
    Feature {
        bit: 8192,
        label: "control MIDI devices",
        setting: None,
    },
    Feature {
        bit: 16384,
        label: "download multiple files",
        setting: None,
    },
    Feature {
        bit: 65536,
        label: "capture your keyboard",
        setting: None,
    },
    Feature {
        bit: 131072,
        label: "lock your mouse pointer",
        setting: None,
    },
    Feature {
        bit: 524288,
        label: "handle a link protocol",
        setting: None,
    },
    Feature {
        bit: 1048576,
        label: "use storage in other sites",
        setting: None,
    },
    Feature {
        bit: 8388608,
        label: "manage your windows",
        setting: None,
    },
    Feature {
        bit: 16777216,
        label: "access your files",
        setting: None,
    },
    Feature {
        bit: 67108864,
        label: "access your local network",
        setting: None,
    },
];

/// `cef_media_access_permission_types_t` (getUserMedia / screen sharing).
pub const MEDIA_FEATURES: &[Feature] = &[
    Feature {
        bit: 1,
        label: "use your microphone",
        setting: Some("content.media.audio_capture"),
    },
    Feature {
        bit: 2,
        label: "use your camera",
        setting: Some("content.media.video_capture"),
    },
    Feature {
        bit: 4,
        label: "capture your desktop audio",
        setting: Some("content.desktop_capture"),
    },
    Feature {
        bit: 8,
        label: "capture your screen",
        setting: Some("content.desktop_capture"),
    },
];

#[derive(Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny,
    /// Ask the user, describing what the site wants.
    Ask(String),
}

/// Answers given with `A`/`N`, per origin and feature, for this session.
#[derive(Default)]
pub struct Remembered {
    answers: HashMap<(String, u32), bool>,
}

impl Remembered {
    pub fn remember(&mut self, origin: &str, bits: u32, features: &[Feature], allow: bool) {
        for feature in features.iter().filter(|f| bits & f.bit != 0) {
            self.answers
                .insert((origin.to_string(), feature.bit), allow);
        }
    }
}

/// Deny if anything requested is refused, allow if everything is granted,
/// otherwise ask. Unknown bits always ask.
pub fn decide(
    origin: &str,
    bits: u32,
    features: &[Feature],
    settings: &Settings,
    remembered: &Remembered,
) -> Decision {
    let requested: Vec<&Feature> = features.iter().filter(|f| bits & f.bit != 0).collect();
    let known: u32 = requested.iter().map(|f| f.bit).fold(0, |a, b| a | b);
    let mut ask = bits & !known != 0;
    for feature in &requested {
        let answer = remembered
            .answers
            .get(&(origin.to_string(), feature.bit))
            .copied()
            .or_else(|| match feature.setting.map(|s| settings.str(s)) {
                Some("true") => Some(true),
                Some("false") => Some(false),
                _ => None,
            });
        match answer {
            Some(false) => return Decision::Deny,
            Some(true) => {}
            None => ask = true,
        }
    }
    if !ask {
        return Decision::Allow;
    }
    let mut labels: Vec<&str> = requested.iter().map(|f| f.label).collect();
    labels.dedup();
    if labels.is_empty() {
        labels.push("use a browser feature");
    }
    Decision::Ask(format!("{origin} wants to {}", join(&labels)))
}

fn join(items: &[&str]) -> String {
    match items {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Value;

    fn settings(pairs: &[(&str, &str)]) -> Settings {
        let mut s = Settings::default();
        for (name, value) in pairs {
            s.set(name, Value::Str(value.to_string())).unwrap();
        }
        s
    }

    #[test]
    fn asks_by_default_and_names_everything() {
        let d = decide(
            "https://meet.example",
            4 | 4096,
            PROMPT_FEATURES,
            &Settings::default(),
            &Remembered::default(),
        );
        assert_eq!(
            d,
            Decision::Ask(
                "https://meet.example wants to use your camera and use your microphone".into()
            )
        );
    }

    #[test]
    fn settings_allow_and_deny() {
        let s = settings(&[
            ("content.geolocation", "true"),
            ("content.notifications.enabled", "false"),
        ]);
        let none = Remembered::default();
        assert_eq!(
            decide("o", 256, PROMPT_FEATURES, &s, &none),
            Decision::Allow
        );
        assert_eq!(
            decide("o", 32768, PROMPT_FEATURES, &s, &none),
            Decision::Deny
        );
        // One refusal denies a combined request.
        assert_eq!(
            decide("o", 256 | 32768, PROMPT_FEATURES, &s, &none),
            Decision::Deny
        );
        // Features without a setting still ask, even next to granted ones.
        assert!(matches!(
            decide("o", 256 | 16, PROMPT_FEATURES, &s, &none),
            Decision::Ask(_)
        ));
        // Bits we don't know about ask.
        assert!(
            matches!(decide("o", 1 << 31, PROMPT_FEATURES, &s, &none), Decision::Ask(m) if m.contains("a browser feature"))
        );
    }

    #[test]
    fn remembered_answers_are_per_origin() {
        let mut r = Remembered::default();
        r.remember("https://a.org", 2, MEDIA_FEATURES, true);
        let s = Settings::default();
        assert_eq!(
            decide("https://a.org", 2, MEDIA_FEATURES, &s, &r),
            Decision::Allow
        );
        assert!(matches!(
            decide("https://b.org", 2, MEDIA_FEATURES, &s, &r),
            Decision::Ask(_)
        ));
        r.remember("https://a.org", 1, MEDIA_FEATURES, false);
        assert_eq!(
            decide("https://a.org", 1 | 2, MEDIA_FEATURES, &s, &r),
            Decision::Deny
        );
    }
}
