//! Bounded semantic lookup. Input hooks only enqueue; no OCR guesses or raw key logging.
use super::*;
use crate::capture::step_pipeline::Caption;
use std::sync::mpsc::{self, Receiver, SyncSender};
#[derive(Clone, Copy)]
pub(super) enum Activity {
    Click,
    Scroll(bool),
    Type,
    Key(&'static str),
}
struct Request {
    point: [i32; 2],
    activity: Activity,
    at: std::time::Instant,
    reply: mpsc::Sender<Caption>,
}
pub(super) fn describe(point: [i32; 2], activity: Activity) -> Receiver<Caption> {
    let (tx, rx) = mpsc::channel();
    if let Some(sender) = broker() {
        let _ = sender.try_send(Request {
            point,
            activity,
            at: std::time::Instant::now(),
            reply: tx,
        });
    }
    rx
}
fn broker() -> Option<&'static SyncSender<Request>> {
    static BROKER: OnceLock<Option<SyncSender<Request>>> = OnceLock::new();
    BROKER.get_or_init(|| {
        let (tx, rx) = mpsc::sync_channel::<Request>(8);
        std::thread::Builder::new().name("clipcraft-step-context".into()).spawn(move || {
            use windows::Win32::{System::Com::{CoInitializeEx, CoUninitialize, CoCreateInstance, COINIT_MULTITHREADED, CLSCTX_INPROC_SERVER}, UI::Accessibility::*};
            let prior = unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
            let _dpi = (!prior.is_null()).then_some(DpiContext(prior));
            let initialized = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok();
            let automation = initialized.then(|| unsafe { CoCreateInstance::<_, IUIAutomation2>(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).ok() }).flatten();
            if let Some(a) = &automation { unsafe { let _=a.SetConnectionTimeout(60); let _=a.SetTransactionTimeout(60); let _=a.SetAutoSetFocus(false); } }
            for r in rx {
                let mut caption = Caption::default();
                if let Activity::Key(key) = r.activity { caption.instruction = Some(format!("Press {key}")); }
                else if r.at.elapsed() < std::time::Duration::from_millis(350) && let Some(a) = &automation {
                    unsafe {
                        let element = if matches!(r.activity, Activity::Type) { a.GetFocusedElement() } else { a.ElementFromPoint(windows::Win32::Foundation::POINT{x:r.point[0],y:r.point[1]}) };
                        if let Ok(mut e) = element {
                            let mut safe = e.CurrentIsPassword().is_ok_and(|p| !p.as_bool());
                            if safe && matches!(r.activity, Activity::Click) {
                                // Only an actionable control whose bounds contain the point can name a click.
                                for _ in 0..4 {
                                    let kind = e.CurrentControlType().ok();
                                    if kind.is_some_and(|k| [UIA_ButtonControlTypeId, UIA_EditControlTypeId, UIA_CheckBoxControlTypeId, UIA_RadioButtonControlTypeId, UIA_MenuItemControlTypeId, UIA_HyperlinkControlTypeId, UIA_ListItemControlTypeId, UIA_TreeItemControlTypeId, UIA_TabItemControlTypeId, UIA_ComboBoxControlTypeId, UIA_SliderControlTypeId].contains(&k)) {
                                        safe &= e.CurrentBoundingRectangle().is_ok_and(|b| r.point[0]>=b.left && r.point[0]<b.right && r.point[1]>=b.top && r.point[1]<b.bottom);
                                        if safe {
                                            use windows::core::Interface;
                                            let label=|value:windows::core::Result<windows::core::BSTR>| value.ok().and_then(|s|capture_core::step_capture::context_label(&s.to_string()));
                                            caption.name=label(e.CurrentName())
                                                .or_else(||label(e.GetCurrentPattern(UIA_LegacyIAccessiblePatternId).and_then(|p|p.cast::<IUIAutomationLegacyIAccessiblePattern>()).and_then(|p|p.CurrentName())))
                                                .or_else(||label(e.CurrentHelpText()));
                                            if caption.name.is_none() {
                                                // Some buttons expose their label as a text child.
                                                if let Ok(walker)=a.ControlViewWalker() {
                                                    let mut child=walker.GetFirstChildElement(&e);
                                                    for _ in 0..6 { let Ok(c)=child else {break;};
                                                        if c.CurrentIsPassword().is_ok_and(|p|!p.as_bool()) && c.CurrentControlType().ok()==Some(UIA_TextControlTypeId) {caption.name=label(c.CurrentName());if caption.name.is_some(){break;}}
                                                        child=walker.GetNextSiblingElement(&c);
                                                    }
                                                }
                                            }
                                            if caption.name.is_none() {
                                                let control=label(e.CurrentLocalizedControlType()).unwrap_or_else(||"control".into());
                                                caption.instruction=Some(format!("Click the {control} shown"));
                                            }
                                        }
                                        break;
                                    }
                                    let parent = a.ControlViewWalker().and_then(|w| w.GetParentElement(&e));
                                    let Ok(parent) = parent else { break; }; e = parent;
                                    if !e.CurrentIsPassword().is_ok_and(|p|!p.as_bool()) { break; }
                                }
                            } else if safe && matches!(r.activity, Activity::Type) && e.CurrentControlType().ok()==Some(UIA_EditControlTypeId) {
                                use windows::core::Interface;
                                if let Ok(value) = e.GetCurrentPattern(UIA_ValuePatternId).and_then(|p|p.cast::<IUIAutomationValuePattern>()).and_then(|p|p.CurrentValue()) {
                                    let value: String = value.to_string().chars().take(240).collect();
                                    let name = e.CurrentName().ok().and_then(|s|capture_core::step_capture::context_label(&s.to_string()));
                                    if !value.is_empty() { caption.instruction=Some(match name { Some(n)=>format!("Type \"{value}\" in the \"{n}\" box"),None=>format!("Type \"{value}\" in the text box") }); }
                                }
                            }
                        }
                    }
                }
                if let Activity::Scroll(down) = r.activity {
                    caption.instruction = Some(if down { "Scroll down to the view shown" } else { "Scroll up to the view shown" }.into());
                }
                if matches!(r.activity,Activity::Click) && caption.name.is_none() && caption.instruction.is_none() {caption.instruction=Some("Click the highlighted location".into());}
                let _ = r.reply.send(caption);
            }
            drop(automation); if initialized { unsafe {CoUninitialize();} }
        }).ok()?;
        Some(tx)
    }).as_ref()
}
pub(super) fn date() -> String {
    let mut time = windows_sys::Win32::Foundation::SYSTEMTIME::default();
    unsafe {
        windows_sys::Win32::System::SystemInformation::GetLocalTime(&mut time);
    }
    format!("{:04}-{:02}-{:02}", time.wYear, time.wMonth, time.wDay)
}
