use capture_core::keybinds::{Binding, Chord, Keybinds};

pub(super) trait Native {
    fn register(&mut self, id: i32, chord: Chord) -> Result<(), String>;
    fn unregister(&mut self, id: i32);
}
#[derive(Default)]
pub(super) struct Registrations {
    active: Vec<(i32, Binding)>,
    staged: Option<Vec<(i32, Binding)>>,
}
impl Registrations {
    pub fn action(&self, id: i32) -> Option<Binding> {
        self.active.iter().find(|(i, _)| *i == id).map(|(_, b)| *b)
    }
    pub fn stage(&mut self, keys: &Keybinds, native: &mut impl Native) -> Result<(), String> {
        keys.validate().map_err(|e| e.to_string())?;
        if self.staged.is_some() {
            return Err("Shortcut changes are already pending".into());
        }
        let mut next = Vec::new();
        for &b in keys.bindings.iter().filter(|_| keys.enabled) {
            if let Some((id, _)) = self.active.iter().find(|(_, old)| old.chord == b.chord) {
                next.push((*id, b));
                continue;
            }
            let id = (1..=32)
                .find(|i| !self.active.iter().chain(next.iter()).any(|(id, _)| id == i))
                .unwrap();
            if let Err(e) = native.register(id, b.chord) {
                for (id, _) in &next {
                    if !self.active.iter().any(|(i, _)| i == id) {
                        native.unregister(*id);
                    }
                }
                return Err(format!("{}: {e}", b.chord.label()));
            }
            next.push((id, b));
        }
        self.staged = Some(next);
        Ok(())
    }
    pub fn finish(&mut self, commit: bool, native: &mut impl Native) {
        if let Some(next) = self.staged.take() {
            let (remove, keep) = if commit {
                (&self.active, &next)
            } else {
                (&next, &self.active)
            };
            for (id, _) in remove {
                if !keep.iter().any(|(i, _)| i == id) {
                    native.unregister(*id);
                }
            }
            if commit {
                self.active = next;
            }
        }
    }
    pub fn clear(&mut self, native: &mut impl Native) {
        self.finish(false, native);
        for (id, _) in self.active.drain(..) {
            native.unregister(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkey::{Registration, apply};
    use capture_core::keybinds::Action;
    #[derive(Default)]
    struct Fake {
        registered: Vec<(i32, Chord)>,
        fail: Option<Chord>,
    }
    impl Native for Fake {
        fn register(&mut self, id: i32, chord: Chord) -> Result<(), String> {
            if self.fail == Some(chord) {
                return Err("Already in use".into());
            }
            assert!(!self.registered.iter().any(|(i, c)| *i == id || *c == chord));
            self.registered.push((id, chord));
            Ok(())
        }
        fn unregister(&mut self, id: i32) {
            assert!(self.registered.iter().any(|(i, _)| *i == id));
            self.registered.retain(|(i, _)| *i != id);
        }
    }
    #[derive(Default)]
    struct Adapter {
        state: Registrations,
        native: Fake,
    }
    impl Registration for Adapter {
        fn stage(&mut self, k: &Keybinds) -> Result<(), String> {
            self.state.stage(k, &mut self.native)
        }
        fn finish(&mut self, c: bool) -> Result<(), String> {
            self.state.finish(c, &mut self.native);
            Ok(())
        }
    }
    #[test]
    fn replacement_is_atomic_across_native_conflict_persistence_failure_reassignment_and_disable() {
        let mut a = Adapter::default();
        let old = Keybinds::default();
        apply(&mut a, &old, || Ok(())).unwrap();
        let registered = a.native.registered.clone();
        let mut next = old.clone();
        next.set(
            Action::Saved,
            Some(Chord {
                key: 0x7a,
                modifiers: 1,
            }),
        );
        next.set(
            Action::Steps,
            Some(Chord {
                key: 0x2c,
                modifiers: 0,
            }),
        );
        a.native.fail = next.get(Action::Steps);
        assert!(apply(&mut a, &next, || panic!("must not save conflicts")).is_err());
        assert_eq!(a.native.registered, registered);
        assert_eq!(a.state.action(1), Some(old.bindings[0]));
        a.native.fail = None;
        assert!(apply(&mut a, &next, || Err("Disk full".into())).is_err());
        assert_eq!(a.native.registered, registered);
        apply(&mut a, &next, || Ok(())).unwrap();
        assert_eq!(a.native.registered.len(), 2);
        assert!(a.state.action(1).is_none());
        let regs = a.native.registered.clone();
        let chord = next.get(Action::Saved);
        next.set(Action::Saved, None);
        next.set(Action::Region, chord);
        apply(&mut a, &next, || Ok(())).unwrap();
        assert_eq!(regs, a.native.registered);
        assert!(
            a.state
                .active
                .iter()
                .any(|(_, b)| b.action == Action::Region)
        );
        next.enabled = false;
        apply(&mut a, &next, || Ok(())).unwrap();
        assert!(a.native.registered.is_empty());
        next.enabled = true;
        apply(&mut a, &next, || Ok(())).unwrap();
        a.state.clear(&mut a.native);
        assert!(a.native.registered.is_empty());
    }
}
