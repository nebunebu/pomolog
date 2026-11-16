//! The pomodoro state machine. It has no clock of its own: every method takes
//! the current wall-clock time, which keeps it deterministic and testable.

use super::model::{Id, Interruption, Phase, SessionRecord};
use chrono::NaiveDateTime;

/// A gap between ticks longer than this is treated as the machine having been
/// asleep, and is not credited as focus time.
const SUSPEND_GAP_MS: i64 = 90_000;

#[derive(Debug, Clone, PartialEq)]
pub struct TimerCfg {
    pub focus_s: i64,
    pub short_s: i64,
    pub long_s: i64,
    pub long_every: u32,
    pub auto_start_breaks: bool,
    pub auto_start_focus: bool,
    /// Flow mode: focus counts up with no fixed end.
    pub flow: bool,
    /// In flow mode, break length as a fraction of the time worked.
    pub flow_break_ratio: f64,
}

impl Default for TimerCfg {
    fn default() -> Self {
        TimerCfg {
            focus_s: 25 * 60,
            short_s: 5 * 60,
            long_s: 15 * 60,
            long_every: 4,
            auto_start_breaks: true,
            auto_start_focus: false,
            flow: false,
            flow_break_ratio: 0.2,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Active {
    pub phase: Phase,
    pub planned_s: i64,
    /// Counting up with no fixed end.
    pub open_ended: bool,
    pub paused: bool,
    pub card_id: Option<Id>,
    /// Active milliseconds in the whole phase.
    pub elapsed_ms: i64,
    /// Active milliseconds since the last card switch.
    seg_elapsed_ms: i64,
    seg_started_at: NaiveDateTime,
    last_tick: NaiveDateTime,
    interruptions: Vec<Interruption>,
}

impl Active {
    pub fn elapsed_s(&self) -> i64 {
        self.elapsed_ms / 1000
    }
    pub fn remaining_s(&self) -> i64 {
        if self.open_ended {
            0
        } else {
            // Round up so the display does not show 0:00 while still running.
            ((self.planned_s * 1000 - self.elapsed_ms).max(0) + 999) / 1000
        }
    }
    /// 0.0 to 1.0. Open-ended phases fill once per planned length.
    pub fn progress(&self) -> f32 {
        if self.planned_s <= 0 {
            return 0.0;
        }
        let p = self.elapsed_ms as f64 / (self.planned_s * 1000) as f64;
        if self.open_ended {
            p.fract() as f32
        } else {
            p.clamp(0.0, 1.0) as f32
        }
    }
    /// Seconds since the last card switch (the part not logged yet).
    pub fn segment_s(&self) -> i64 {
        self.seg_elapsed_ms / 1000
    }
    pub fn interruption_count(&self) -> usize {
        self.interruptions.len()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// Persist this record.
    Log(SessionRecord),
    Started(Phase),
    /// A phase ended; `completed` is false when it was skipped or abandoned.
    Ended {
        phase: Phase,
        completed: bool,
    },
}

#[derive(Debug, Clone)]
pub struct Timer {
    pub cfg: TimerCfg,
    pub active: Option<Active>,
    /// The phase that starts next when idle.
    pub next: Phase,
    /// Completed focus phases since the last long break.
    pub cycle: u32,
    /// Card to attach to the next focus phase.
    pub card_id: Option<Id>,
    /// Break length override set by finishing a flow session.
    next_break_s: Option<i64>,
}

impl Timer {
    pub fn new(cfg: TimerCfg) -> Self {
        Timer {
            cfg,
            active: None,
            next: Phase::Focus,
            cycle: 0,
            card_id: None,
            next_break_s: None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.active.as_ref().is_some_and(|a| !a.paused)
    }

    /// The phase currently shown: the active one, or the one queued up.
    pub fn phase(&self) -> Phase {
        self.active.as_ref().map_or(self.next, |a| a.phase)
    }

    pub fn planned_for(&self, phase: Phase) -> i64 {
        match phase {
            Phase::Focus => self.cfg.focus_s,
            Phase::ShortBreak => self.next_break_s.unwrap_or(self.cfg.short_s),
            Phase::LongBreak => self.next_break_s.unwrap_or(self.cfg.long_s),
        }
    }

    pub fn start(&mut self, now: NaiveDateTime) -> Vec<Effect> {
        if self.active.is_some() {
            return self.resume(now);
        }
        let phase = self.next;
        let planned_s = self.planned_for(phase).max(1);
        self.next_break_s = None;
        self.active = Some(Active {
            phase,
            planned_s,
            open_ended: phase == Phase::Focus && self.cfg.flow,
            paused: false,
            card_id: if phase == Phase::Focus {
                self.card_id
            } else {
                None
            },
            elapsed_ms: 0,
            seg_elapsed_ms: 0,
            seg_started_at: now,
            last_tick: now,
            interruptions: Vec::new(),
        });
        vec![Effect::Started(phase)]
    }

    pub fn pause(&mut self, now: NaiveDateTime, reason: &str) -> Vec<Effect> {
        self.advance(now);
        if let Some(a) = self.active.as_mut()
            && !a.paused
        {
            a.paused = true;
            if a.phase == Phase::Focus {
                a.interruptions.push(Interruption {
                    at: now,
                    kind: "pause".into(),
                    reason: reason.into(),
                });
            }
        }
        Vec::new()
    }

    pub fn resume(&mut self, now: NaiveDateTime) -> Vec<Effect> {
        if let Some(a) = self.active.as_mut()
            && a.paused
        {
            a.paused = false;
            a.last_tick = now;
        }
        Vec::new()
    }

    pub fn toggle(&mut self, now: NaiveDateTime) -> Vec<Effect> {
        match &self.active {
            None => self.start(now),
            Some(a) if a.paused => self.resume(now),
            Some(_) => self.pause(now, ""),
        }
    }

    /// Attach a reason to the most recent interruption.
    pub fn set_last_reason(&mut self, reason: &str) {
        if let Some(i) = self
            .active
            .as_mut()
            .and_then(|a| a.interruptions.last_mut())
        {
            i.reason = reason.into();
        }
    }

    /// Add or remove planned time on the running phase.
    pub fn extend(&mut self, secs: i64) {
        if let Some(a) = self.active.as_mut() {
            a.planned_s = (a.planned_s + secs).max(a.elapsed_s() + 1);
        }
    }

    /// Change the card being worked on. While focusing, the time so far is
    /// logged against the previous card.
    pub fn set_card(&mut self, now: NaiveDateTime, card: Option<Id>) -> Vec<Effect> {
        self.card_id = card;
        self.advance(now);
        let mut out = Vec::new();
        if let Some(a) = self.active.as_mut()
            && a.phase == Phase::Focus
            && a.card_id != card
        {
            if a.seg_elapsed_ms >= 1000 {
                out.push(Effect::Log(SessionRecord {
                    kind: a.phase,
                    card_id: a.card_id,
                    started_at: a.seg_started_at,
                    ended_at: now,
                    duration_s: a.seg_elapsed_ms / 1000,
                    planned_s: a.planned_s,
                    completed: false,
                    interruptions: std::mem::take(&mut a.interruptions),
                }));
            }
            a.card_id = card;
            a.seg_elapsed_ms = 0;
            a.seg_started_at = now;
        }
        out
    }

    /// End the current phase early and move on to the next one.
    pub fn skip(&mut self, now: NaiveDateTime) -> Vec<Effect> {
        self.advance(now);
        match self.active.as_ref().map(|a| a.phase) {
            None => {
                // Idle: just rotate which phase is queued.
                self.next = if self.next == Phase::Focus {
                    self.break_phase()
                } else {
                    Phase::Focus
                };
                self.next_break_s = None;
                Vec::new()
            }
            Some(Phase::Focus) => self.end(now, false, Some("skip")),
            Some(_) => self.end(now, false, None),
        }
    }

    /// Give up on the current focus phase. The next phase is focus again.
    pub fn abandon(&mut self, now: NaiveDateTime, reason: &str) -> Vec<Effect> {
        self.advance(now);
        let Some(phase) = self.active.as_ref().map(|a| a.phase) else {
            return Vec::new();
        };
        let mut out = self.end(now, false, Some("abandon"));
        if let Some(Effect::Log(r)) = out.first_mut()
            && let Some(i) = r.interruptions.last_mut()
        {
            i.reason = reason.into();
        }
        if phase == Phase::Focus {
            self.next = Phase::Focus;
        }
        // Abandoning never auto-starts anything.
        if matches!(out.last(), Some(Effect::Started(_))) {
            out.pop();
            self.active = None;
        }
        out
    }

    /// Finish an open-ended focus phase as completed.
    pub fn finish(&mut self, now: NaiveDateTime) -> Vec<Effect> {
        self.advance(now);
        match &self.active {
            Some(a) if a.phase == Phase::Focus => {
                let worked = a.elapsed_s();
                let out = self.end(now, true, None);
                let brk = ((worked as f64 * self.cfg.flow_break_ratio) as i64).max(60);
                if self.cfg.flow {
                    match self.active.as_mut() {
                        Some(b) if b.phase.is_break() => b.planned_s = brk,
                        _ => self.next_break_s = Some(brk),
                    }
                }
                out
            }
            _ => Vec::new(),
        }
    }

    /// Advance the clock. Returns effects when a phase runs out.
    pub fn tick(&mut self, now: NaiveDateTime) -> Vec<Effect> {
        self.advance(now);
        match &self.active {
            Some(a) if !a.open_ended && !a.paused && a.elapsed_ms >= a.planned_s * 1000 => {
                self.end(now, true, None)
            }
            _ => Vec::new(),
        }
    }

    fn break_phase(&self) -> Phase {
        if self.cfg.long_every > 0 && self.cycle >= self.cfg.long_every {
            Phase::LongBreak
        } else {
            Phase::ShortBreak
        }
    }

    fn advance(&mut self, now: NaiveDateTime) {
        let Some(a) = self.active.as_mut() else {
            return;
        };
        if a.paused {
            a.last_tick = now;
            return;
        }
        let dt = (now - a.last_tick).num_milliseconds();
        a.last_tick = now;
        if dt <= 0 {
            return;
        }
        if dt > SUSPEND_GAP_MS {
            if a.phase == Phase::Focus {
                a.interruptions.push(Interruption {
                    at: now,
                    kind: "suspend".into(),
                    reason: String::new(),
                });
            }
            return;
        }
        let room = if a.open_ended {
            i64::MAX
        } else {
            (a.planned_s * 1000 - a.elapsed_ms).max(0)
        };
        let dt = dt.min(room);
        a.elapsed_ms += dt;
        a.seg_elapsed_ms += dt;
    }

    fn end(
        &mut self,
        now: NaiveDateTime,
        completed: bool,
        interruption: Option<&str>,
    ) -> Vec<Effect> {
        let Some(mut a) = self.active.take() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(kind) = interruption {
            a.interruptions.push(Interruption {
                at: now,
                kind: kind.into(),
                reason: String::new(),
            });
        }
        if a.seg_elapsed_ms >= 1000 || completed {
            out.push(Effect::Log(SessionRecord {
                kind: a.phase,
                card_id: a.card_id,
                started_at: a.seg_started_at,
                ended_at: now,
                duration_s: a.seg_elapsed_ms / 1000,
                planned_s: a.planned_s,
                completed,
                interruptions: a.interruptions,
            }));
        }
        out.push(Effect::Ended {
            phase: a.phase,
            completed,
        });

        let auto = match a.phase {
            Phase::Focus => {
                if completed {
                    self.cycle += 1;
                }
                self.next = self.break_phase();
                self.cfg.auto_start_breaks
            }
            Phase::LongBreak => {
                self.cycle = 0;
                self.next = Phase::Focus;
                self.cfg.auto_start_focus
            }
            Phase::ShortBreak => {
                self.next = Phase::Focus;
                self.cfg.auto_start_focus
            }
        };
        // A skipped break should not immediately start focus unless asked to.
        if auto && (completed || a.phase == Phase::Focus) {
            out.extend(self.start(now));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, NaiveDate};

    fn t0() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 16)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap()
    }

    /// Tick once a second for `secs` seconds, collecting effects.
    fn run(t: &mut Timer, from: NaiveDateTime, secs: i64) -> (NaiveDateTime, Vec<Effect>) {
        let mut out = Vec::new();
        let mut now = from;
        for _ in 0..secs {
            now += Duration::seconds(1);
            out.extend(t.tick(now));
        }
        (now, out)
    }

    fn logs(effects: &[Effect]) -> Vec<&SessionRecord> {
        effects
            .iter()
            .filter_map(|e| {
                if let Effect::Log(r) = e {
                    Some(r)
                } else {
                    None
                }
            })
            .collect()
    }

    fn cfg() -> TimerCfg {
        TimerCfg {
            focus_s: 60,
            short_s: 10,
            long_s: 30,
            long_every: 2,
            ..Default::default()
        }
    }

    #[test]
    fn full_cycle_reaches_long_break() {
        let mut t = Timer::new(TimerCfg {
            auto_start_focus: true,
            ..cfg()
        });
        t.start(t0());
        let (now, fx) = run(&mut t, t0(), 60);
        let l = logs(&fx);
        assert_eq!(l.len(), 1);
        assert!(l[0].completed);
        assert_eq!(l[0].duration_s, 60);
        assert_eq!(t.phase(), Phase::ShortBreak);
        assert!(t.is_running(), "break auto-starts");

        let (now, _) = run(&mut t, now, 10);
        assert_eq!(t.phase(), Phase::Focus);
        let (_, _) = run(&mut t, now, 60);
        assert_eq!(t.phase(), Phase::LongBreak);
        assert_eq!(t.active.as_ref().unwrap().planned_s, 30);
    }

    #[test]
    fn long_break_resets_cycle() {
        let mut t = Timer::new(TimerCfg {
            auto_start_focus: true,
            ..cfg()
        });
        t.start(t0());
        let (now, _) = run(&mut t, t0(), 60 + 10 + 60 + 30);
        assert_eq!(t.cycle, 0);
        assert_eq!(t.phase(), Phase::Focus);
        let (_, _) = run(&mut t, now, 60);
        assert_eq!(t.phase(), Phase::ShortBreak);
    }

    #[test]
    fn focus_does_not_auto_start_by_default() {
        let mut t = Timer::new(cfg());
        t.start(t0());
        run(&mut t, t0(), 70);
        assert!(t.active.is_none());
        assert_eq!(t.next, Phase::Focus);
    }

    #[test]
    fn pause_stops_the_clock_and_logs_an_interruption() {
        let mut t = Timer::new(cfg());
        t.start(t0());
        let (now, _) = run(&mut t, t0(), 20);
        t.pause(now, "phone");
        let later = now + Duration::minutes(10);
        t.tick(later);
        assert_eq!(t.active.as_ref().unwrap().elapsed_s(), 20);
        t.resume(later);
        let (_, fx) = run(&mut t, later, 40);
        let l = logs(&fx);
        assert_eq!(l[0].duration_s, 60);
        assert_eq!(l[0].interruptions.len(), 1);
        assert_eq!(l[0].interruptions[0].reason, "phone");
        assert!(l[0].completed);
    }

    #[test]
    fn suspend_gap_is_not_credited() {
        let mut t = Timer::new(cfg());
        t.start(t0());
        let (now, _) = run(&mut t, t0(), 10);
        let woke = now + Duration::hours(3);
        assert!(t.tick(woke).is_empty());
        let a = t.active.as_ref().unwrap();
        assert_eq!(a.elapsed_s(), 10);
        assert_eq!(a.interruption_count(), 1);
    }

    #[test]
    fn skip_focus_logs_incomplete_and_moves_to_break() {
        let mut t = Timer::new(TimerCfg {
            auto_start_breaks: false,
            ..cfg()
        });
        t.start(t0());
        let (now, _) = run(&mut t, t0(), 15);
        let fx = t.skip(now);
        let l = logs(&fx);
        assert_eq!(l[0].duration_s, 15);
        assert!(!l[0].completed);
        assert_eq!(l[0].interruptions[0].kind, "skip");
        assert_eq!(t.next, Phase::ShortBreak);
        assert_eq!(
            t.cycle, 0,
            "a skipped focus does not count towards the long break"
        );
    }

    #[test]
    fn abandon_keeps_focus_next_and_never_auto_starts() {
        let mut t = Timer::new(cfg());
        t.start(t0());
        let (now, _) = run(&mut t, t0(), 15);
        let fx = t.abandon(now, "meeting");
        assert_eq!(logs(&fx)[0].interruptions[0].reason, "meeting");
        assert!(t.active.is_none());
        assert_eq!(t.next, Phase::Focus);
    }

    #[test]
    fn switching_card_splits_the_time() {
        let mut t = Timer::new(cfg());
        t.card_id = Some(1);
        t.start(t0());
        let (now, _) = run(&mut t, t0(), 25);
        let fx = t.set_card(now, Some(2));
        let l = logs(&fx);
        assert_eq!(
            (l[0].card_id, l[0].duration_s, l[0].completed),
            (Some(1), 25, false)
        );
        let (_, fx) = run(&mut t, now, 35);
        let l = logs(&fx);
        assert_eq!(
            (l[0].card_id, l[0].duration_s, l[0].completed),
            (Some(2), 35, true)
        );
    }

    #[test]
    fn extend_adds_time() {
        let mut t = Timer::new(cfg());
        t.start(t0());
        let (now, _) = run(&mut t, t0(), 59);
        t.extend(30);
        let (_, fx) = run(&mut t, now, 10);
        assert!(fx.is_empty());
        assert_eq!(t.active.as_ref().unwrap().remaining_s(), 21);
    }

    #[test]
    fn flow_counts_up_and_sizes_the_break() {
        let mut t = Timer::new(TimerCfg {
            flow: true,
            flow_break_ratio: 0.5,
            ..cfg()
        });
        t.start(t0());
        let (now, fx) = run(&mut t, t0(), 200);
        assert!(fx.is_empty(), "flow never ends on its own");
        let fx = t.finish(now);
        let l = logs(&fx);
        assert_eq!(l[0].duration_s, 200);
        assert!(l[0].completed);
        assert_eq!(t.active.as_ref().unwrap().planned_s, 100);
    }

    #[test]
    fn idle_skip_rotates_the_queued_phase() {
        let mut t = Timer::new(cfg());
        t.skip(t0());
        assert_eq!(t.next, Phase::ShortBreak);
        t.skip(t0());
        assert_eq!(t.next, Phase::Focus);
    }
}
