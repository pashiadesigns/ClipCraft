use std::time::{Duration, Instant};

#[derive(Debug, PartialEq, Eq)]
pub(super) enum State {
    Waiting(u32),
    Ready,
    Cancelled,
}

pub(super) struct Countdown {
    deadline: Instant,
    terminal: Option<State>,
}
impl Countdown {
    pub(super) fn new(seconds: u32, now: Instant) -> Result<Self, String> {
        if ![0, 3, 5, 10].contains(&seconds) {
            return Err("Pick a Start delay: 0, 3, 5 or 10 seconds.".into());
        }
        Ok(Self {
            deadline: now + Duration::from_secs(u64::from(seconds)),
            terminal: None,
        })
    }
    pub(super) fn poll(&mut self, now: Instant, cancel: bool) -> State {
        match self.terminal {
            Some(State::Cancelled) => return State::Cancelled,
            Some(State::Ready) => return State::Ready,
            _ => (),
        }
        if cancel {
            self.terminal = Some(State::Cancelled);
            return State::Cancelled;
        }
        let remaining = self.deadline.saturating_duration_since(now);
        if remaining.is_zero() {
            self.terminal = Some(State::Ready);
            State::Ready
        } else {
            State::Waiting(remaining.as_nanos().div_ceil(1_000_000_000) as u32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn countdown_uses_elapsed_time_and_cancellation_wins_at_deadline() {
        let now = Instant::now();
        let mut c = Countdown::new(5, now).unwrap();
        assert_eq!(c.poll(now, false), State::Waiting(5));
        assert_eq!(
            c.poll(now + Duration::from_millis(1001), false),
            State::Waiting(4)
        );
        assert_eq!(
            c.poll(now + Duration::from_millis(4999), false),
            State::Waiting(1)
        );
        assert_eq!(
            c.poll(
                now + Duration::from_secs(5) - Duration::from_nanos(1),
                false
            ),
            State::Waiting(1)
        );
        assert_eq!(c.poll(now + Duration::from_secs(5), true), State::Cancelled);
        assert_eq!(
            c.poll(now + Duration::from_secs(20), false),
            State::Cancelled
        );
        let mut c = Countdown::new(3, now).unwrap();
        assert_eq!(c.poll(now + Duration::from_secs(10), false), State::Ready);
        assert_eq!(c.poll(now + Duration::from_secs(11), true), State::Ready);
        assert_eq!(
            Countdown::new(0, now).unwrap().poll(now, false),
            State::Ready
        );
        assert!(Countdown::new(u32::MAX, now).is_err());
        assert!(Countdown::new(2, now).is_err());
    }
}
