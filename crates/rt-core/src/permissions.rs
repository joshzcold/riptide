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
        setting: Some("content.mouse_lock"),
    },
    Feature {
        bit: 524288,
        label: "handle a link protocol",
        setting: Some("content.register_protocol_handler"),
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

/// Answers given with `A`/`N` this session, per origin and feature (they are
/// also saved as per-site settings).
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
            .or_else(
                || match feature.setting.map(|s| settings.str_for(s, origin)) {
                    Some("true") => Some(true),
                    Some("false") => Some(false),
                    _ => None,
                },
            );
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

/// One thing a site asks for, as a permission question shows it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Ask {
    /// Which icon to draw, e.g. `camera`; `other` for the rest.
    pub icon: &'static str,
    /// A short name, e.g. `Camera`.
    pub name: &'static str,
}

/// Short names and icons for what the features in `bits` ask for.
pub fn asks(bits: u32, features: &[Feature]) -> Vec<Ask> {
    let ask = |icon, name| Ask { icon, name };
    let mut out: Vec<Ask> = features
        .iter()
        .filter(|f| bits & f.bit != 0)
        .map(|f| match f.label {
            "use your camera" => ask("camera", "Camera"),
            "use your microphone" => ask("microphone", "Microphone"),
            "know your location" => ask("location", "Location"),
            "show notifications" => ask("notifications", "Notifications"),
            "capture your screen" => ask("screen", "Screen"),
            "capture your desktop audio" => ask("screen", "Desktop audio"),
            "read your clipboard" => ask("clipboard", "Clipboard"),
            "pan, tilt and zoom your camera" => ask("camera", "Camera movement"),
            "lock your mouse pointer" => ask("pointer", "Mouse pointer"),
            "capture your keyboard" => ask("keyboard", "Keyboard"),
            "handle a link protocol" => ask("link", "Opening links"),
            "download multiple files" => ask("download", "Several downloads"),
            "access your files" => ask("files", "Files"),
            "use your local fonts" => ask("other", "Fonts"),
            "know when you're idle" => ask("other", "Idle detection"),
            "control MIDI devices" => ask("other", "MIDI devices"),
            "use storage in other sites" => ask("other", "Storage in other sites"),
            "manage your windows" => ask("other", "Window placement"),
            "access your local network" => ask("other", "Local network"),
            _ => ask("other", "Something else"),
        })
        .collect();
    out.dedup();
    out
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
    fn asks_name_and_draw_each_feature() {
        let names: Vec<_> = asks(1 | 2, MEDIA_FEATURES)
            .into_iter()
            .map(|a| (a.icon, a.name))
            .collect();
        assert_eq!(names, [("microphone", "Microphone"), ("camera", "Camera")]);
        let every = PROMPT_FEATURES.iter().fold(0, |b, f| b | f.bit);
        assert!(
            asks(every, PROMPT_FEATURES)
                .iter()
                .all(|a| a.name != "Something else")
        );
        let media = MEDIA_FEATURES.iter().fold(0, |b, f| b | f.bit);
        assert!(
            asks(media, MEDIA_FEATURES)
                .iter()
                .all(|a| a.name != "Something else")
        );
    }

    #[test]
    fn mouse_lock_and_protocol_handlers_follow_their_settings() {
        let s = settings(&[
            ("content.mouse_lock", "true"),
            ("content.register_protocol_handler", "false"),
        ]);
        let none = Remembered::default();
        assert_eq!(
            decide("o", 131072, PROMPT_FEATURES, &s, &none),
            Decision::Allow
        );
        assert_eq!(
            decide("o", 524288, PROMPT_FEATURES, &s, &none),
            Decision::Deny
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
    fn per_site_settings_decide_for_their_site() {
        let mut s = Settings::default();
        s.set_for(
            "https://meet.example",
            "content.media.video_capture",
            Value::Str("true".into()),
        )
        .unwrap();
        let none = Remembered::default();
        assert_eq!(
            decide("https://meet.example", 4, PROMPT_FEATURES, &s, &none),
            Decision::Allow
        );
        assert!(matches!(
            decide("https://other.example", 4, PROMPT_FEATURES, &s, &none),
            Decision::Ask(_)
        ));
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
