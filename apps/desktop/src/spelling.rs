//! Read-only local Windows spelling. App corrections never change OS dictionaries.
use std::ops::Range;
#[derive(Clone, Debug)]
pub(super) struct Issue {
    pub word: String,
    pub range: Range<usize>,
    pub suggestions: Vec<String>,
}
#[cfg(windows)]
pub(super) fn check(text: &str, language: &str) -> Result<Vec<Issue>, String> {
    use windows::{
        Win32::{
            Globalization::{ISpellCheckerFactory, SpellCheckerFactory},
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
                CoTaskMemFree, CoUninitialize,
            },
        },
        core::{PCWSTR, PWSTR},
    };
    if text.chars().count() > 4096 || language.len() > 64 || language.contains('\0') {
        return Err("Choose a valid language tag and a text box of up to 4096 characters.".into());
    }
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .map_err(|e| e.to_string())?;
    struct Runtime;
    impl Drop for Runtime {
        fn drop(&mut self) {
            unsafe {
                CoUninitialize();
            }
        }
    }
    let _runtime = Runtime;
    let result = (|| -> windows::core::Result<Vec<Issue>> {
        unsafe {
            let factory: ISpellCheckerFactory =
                CoCreateInstance(&SpellCheckerFactory, None, CLSCTX_INPROC_SERVER)?;
            let language: Vec<_> = language.encode_utf16().chain([0]).collect();
            let checker = factory.CreateSpellChecker(PCWSTR(language.as_ptr()))?;
            let input: Vec<_> = text.encode_utf16().chain([0]).collect();
            let errors = checker.Check(PCWSTR(input.as_ptr()))?;
            let mut offsets = vec![0];
            for (i, c) in text.chars().enumerate() {
                for _ in 0..c.len_utf16() {
                    offsets.push(i + 1);
                }
            }
            let mut issues = Vec::new();
            for _ in 0..128 {
                let mut error = None;
                errors.Next(&mut error).ok()?;
                let Some(error) = error else {
                    break;
                };
                let a = error.StartIndex()? as usize;
                let b = a + error.Length()? as usize;
                if b >= offsets.len() {
                    continue;
                }
                let range = offsets[a]..offsets[b];
                let word = text
                    .chars()
                    .skip(range.start)
                    .take(range.len())
                    .collect::<String>();
                let value: Vec<_> = word.encode_utf16().chain([0]).collect();
                let suggestions = checker.Suggest(PCWSTR(value.as_ptr()))?;
                let mut values = Vec::new();
                for _ in 0..6 {
                    let mut ptr = [PWSTR::null()];
                    let mut fetched = 0;
                    suggestions.Next(&mut ptr, Some(&mut fetched)).ok()?;
                    if fetched == 0 {
                        break;
                    }
                    let value = ptr[0].to_string();
                    CoTaskMemFree(Some(ptr[0].0.cast()));
                    if let Ok(value) = value {
                        values.push(value);
                    }
                }
                issues.push(Issue {
                    word,
                    range,
                    suggestions: values,
                });
            }
            Ok(issues)
        }
    })();
    result.map_err(|_|"Windows could not check this language. Install its spelling dictionary in Windows language settings, or choose another language tag.".into())
}
#[cfg(not(windows))]
pub(super) fn check(_text: &str, _language: &str) -> Result<Vec<Issue>, String> {
    Err("Local spelling currently requires Windows.".into())
}

#[cfg(all(test, windows))]
mod tests {
    #[test]
    #[ignore = "Opt-in local Windows dictionary check; no OS dictionary modification"]
    fn local_dictionary_returns_bounded_suggestions() {
        let issues = super::check("A mispelled example.", "en-US").unwrap();
        assert!(
            issues
                .iter()
                .any(|i| i.word == "mispelled" && i.range == (2..11))
        );
        assert!(super::check("test", "invalid-language-tag").is_err());
    }
}
