//! Settings › Voice › Microphone (LOO-454): which of the system's inputs dictation and
//! live mode record from, with a meter that shows it's hearing, and the mic's name in
//! the composer mic's tooltip. The pick is the device's own, kept in prefs.json, as Hold
//! to talk is; one that isn't plugged in leaves loom on the system's default, and says so.

use super::*;
use crate::model::voice::{Menu as VoiceMenu, Tab};
use crate::voice::{Input, Mic};

/// How often the inputs are listed again while the row shows, to see a mic plugged in
/// or out.
const RELIST: Duration = Duration::from_secs(2);
/// The meter: a thin bar this wide.
const METER: f32 = 56.0;

/// The words for the system's default in the picker: "System default (NAME)".
fn default_label(name: Option<&str>) -> String {
    match name {
        Some(n) => format!("System default ({n})"),
        None => "System default".into(),
    }
}

/// The picker's choices: the default first, each input, and the picked one at the end
/// while it isn't plugged in, so what's picked is always there.
fn choices(inputs: &crate::voice::Inputs, picked: Option<&Input>) -> Vec<(String, String)> {
    let mut out = vec![(String::new(), default_label(inputs.default.as_deref()))];
    out.extend(inputs.all.iter().map(|i| (i.id.clone(), i.name.clone())));
    if let Some(p) = picked.filter(|p| !inputs.has(&p.id)) {
        out.push((p.id.clone(), format!("{}, not plugged in", p.name)));
    }
    out
}

/// The line under the row: why the meter's mic didn't start, or that the picked mic
/// isn't plugged in, once the inputs are known.
fn mic_line(
    inputs: &crate::voice::Inputs,
    listed: bool,
    picked: Option<&Input>,
    why: Option<&str>,
) -> Option<String> {
    if let Some(why) = why {
        return Some(why.to_string());
    }
    let p = picked?;
    (listed && !inputs.has(&p.id))
        .then(|| format!("{} isn't plugged in, so the default is used.", p.name))
}

impl Loom {
    /// Starts the mic dictation or live mode records from: the picked one, or the
    /// default while it isn't plugged in, which loom says once.
    pub(super) fn start_mic(&mut self, cx: &mut Context<Self>) -> Result<Mic, String> {
        let picked = self.prefs.mic.clone();
        let mic = Mic::start(picked.as_ref().map(|m| m.id.as_str()))?;
        match picked {
            Some(p) if mic.fell_back => {
                if self.missing_said.as_deref() != Some(p.id.as_str()) {
                    self.model.notices.push(format!(
                        "{} isn't plugged in, so loom is using {}.",
                        p.name, mic.name
                    ));
                    self.take_notice(cx);
                    self.missing_said = Some(p.id);
                }
            }
            _ => self.missing_said = None,
        }
        Ok(mic)
    }

    /// Whether Settings › Voice shows the Microphone row: on Dictation and Live.
    fn mic_row_shown(&self) -> bool {
        self.popup == Some(Screen::Voice)
            && self.model.voice.came
            && matches!(self.model.voice.tab, Tab::Dictation | Tab::Live)
    }

    /// Each tick: the inputs listed once at the start, for the mic's tooltip, and again
    /// every couple of seconds while the row shows; and the meter's mic open while the
    /// row shows and nothing else records, and how loud it hears.
    pub(super) fn tick_mics(&mut self, cx: &mut Context<Self>) {
        if let Some(coming) = &self.inputs_coming {
            if let Ok(inputs) = coming.try_recv() {
                self.inputs_coming = None;
                if inputs != self.inputs {
                    self.inputs = inputs;
                    cx.notify();
                }
            }
        }
        let shown = self.mic_row_shown();
        let due = self
            .inputs_asked
            .is_none_or(|at| shown && at.elapsed() >= RELIST);
        if due && self.inputs_coming.is_none() {
            self.inputs_coming = Some(crate::voice::list_inputs());
            self.inputs_asked = Some(Instant::now());
        }
        if !shown || self.mic.is_some() || self.live_mic.is_some() {
            if self.meter.take().is_some() || self.meter_why.take().is_some() {
                self.meter_level = 0.0;
                cx.notify();
            }
            return;
        }
        // The picked mic came back while the meter heard the default: it hears that one.
        let back = self.meter.as_ref().is_some_and(|m| m.fell_back)
            && self
                .prefs
                .mic
                .as_ref()
                .is_some_and(|p| self.inputs.has(&p.id));
        if back {
            self.meter = None;
        }
        if self.meter.is_none() && self.meter_why.is_none() {
            let picked = self.prefs.mic.as_ref().map(|m| m.id.clone());
            match Mic::start(picked.as_deref()) {
                Ok(mic) => self.meter = Some(mic),
                Err(why) => {
                    self.meter_why = Some(why);
                    cx.notify();
                }
            }
        }
        let Some(meter) = &self.meter else {
            return;
        };
        let mut heard = None;
        for pcm in meter.chunks.try_iter() {
            heard = Some(crate::voice::level(&pcm));
        }
        if let Some(now) = heard {
            // Up at once, down more slowly, so it doesn't flicker.
            let level = if now >= self.meter_level {
                now
            } else {
                self.meter_level * 0.6 + now * 0.4
            };
            if (level - self.meter_level).abs() > 0.005 {
                self.meter_level = level;
                cx.notify();
            }
        }
    }

    /// The picker's pick: `id` empty for the system's default. The meter hears it next.
    pub(super) fn pick_mic(
        &mut self,
        id: &str,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mic = (!id.is_empty()).then(|| Input {
            id: id.to_string(),
            name: self
                .inputs
                .all
                .iter()
                .find(|i| i.id == id)
                .map_or_else(|| name.to_string(), |i| i.name.clone()),
        });
        if mic.is_some() && self.prefs.mic.as_ref().map(|m| &m.id) == mic.as_ref().map(|m| &m.id) {
            // The one picked already, maybe not plugged in: it keeps its name.
            return;
        }
        self.set_prefs(|p| p.mic = mic, window, cx);
        self.meter = None;
        self.meter_why = None;
        self.meter_level = 0.0;
        self.missing_said = None;
    }

    /// The composer mic's tooltip: what it does and the mic it records from, the one
    /// recording while it does.
    pub(super) fn mic_tip(&self, hold: bool) -> String {
        let what = if hold { "Hold to dictate" } else { "Dictate" };
        let name = match &self.mic {
            Some(m) => Some(m.name.clone()),
            None => self.inputs.in_use(self.prefs.mic.as_ref()),
        };
        match name {
            Some(n) => format!("{what} with {n}"),
            None => what.to_string(),
        }
    }

    /// Settings › Voice's Microphone row: the meter and the picker, with the system's
    /// default first.
    pub(super) fn mic_row(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.p;
        let picked = self.prefs.mic.as_ref();
        let listed = self.inputs_asked.is_some() && self.inputs_coming.is_none()
            || !self.inputs.all.is_empty();
        let now = match picked {
            Some(m) => m.name.clone(),
            None => default_label(self.inputs.default.as_deref()),
        };
        let picker = self.voice_picker(
            VoiceMenu::Microphone,
            now,
            choices(&self.inputs, picked),
            picked.map(|m| m.id.clone()).unwrap_or_default(),
            cx,
        );
        let level = if self.meter.is_some() {
            self.meter_level
        } else {
            0.0
        };
        let meter = div()
            .id("voice-mic-level")
            .role(Role::Meter)
            .aria_label("Level")
            .w(px(METER))
            .h(px(3.0))
            .rounded_full()
            .bg(p.line)
            .child(div().h_full().w(relative(level)).rounded_full().bg(p.ink));
        self.row(
            None,
            "Microphone",
            mic_line(&self.inputs, listed, picked, self.meter_why.as_deref()),
            Some(
                div()
                    .flex()
                    .items_center()
                    .gap(px(space::M))
                    .child(meter)
                    .child(picker)
                    .into_any_element(),
            ),
        )
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{choices, mic_line};
    use crate::voice::{Input, Inputs};

    fn usb() -> Input {
        Input {
            id: "wasapi:usb".into(),
            name: "USB Mic".into(),
        }
    }

    #[test]
    fn the_default_comes_first_with_its_name() {
        let inputs = Inputs {
            default: Some("Laptop Mic".into()),
            all: vec![
                Input {
                    id: "wasapi:laptop".into(),
                    name: "Laptop Mic".into(),
                },
                usb(),
            ],
        };
        let c = choices(&inputs, None);
        assert_eq!(c[0], (String::new(), "System default (Laptop Mic)".into()));
        assert_eq!(c.len(), 3);
        assert_eq!(c[2].1, "USB Mic");
        assert_eq!(choices(&Inputs::default(), None)[0].1, "System default");
    }

    #[test]
    fn a_picked_mic_unplugged_stays_in_the_list_and_is_said() {
        let inputs = Inputs {
            default: Some("Laptop Mic".into()),
            all: Vec::new(),
        };
        let c = choices(&inputs, Some(&usb()));
        assert_eq!(c.last().unwrap().1, "USB Mic, not plugged in");
        assert_eq!(
            mic_line(&inputs, true, Some(&usb()), None).as_deref(),
            Some("USB Mic isn't plugged in, so the default is used.")
        );
        // Before the inputs are listed, it isn't said.
        assert_eq!(mic_line(&inputs, false, Some(&usb()), None), None);
        assert_eq!(mic_line(&inputs, true, None, None), None);
        assert_eq!(
            mic_line(&inputs, true, None, Some("There's no mic.")).as_deref(),
            Some("There's no mic.")
        );
    }
}
