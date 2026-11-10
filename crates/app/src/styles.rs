//! Stilul fiecarei dictari: starea comuna intre daemon (care afla aplicatia din fata si setarile)
//! si curatarea de pe firul de transcriere (care alege stilul si asambleaza promptul).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use vf_application::{Cleaned, TextCleaner};
use vf_domain::styles::{self, AppRef, CleanupLevel, RecentApps, StyleId, StylesSettings};
use vf_domain::Settings;

/// Ce stil a primit ultima dictare, pentru interfata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    pub style: StyleId,
    pub level: CleanupLevel,
    pub app: Option<AppRef>,
    /// `false`: textul nu a trecut prin model (stilul "Code").
    pub cleaned: bool,
}

#[derive(Debug, Default)]
pub struct State {
    /// Regulile de baza din `cleanup.prompt`.
    pub base_prompt: String,
    pub level: CleanupLevel,
    pub styles: StylesSettings,
    /// Aplicatia din fata, asa cum a anuntat-o interfata ultima data.
    pub current_app: Option<AppRef>,
    pub recent: RecentApps,
    pub last_applied: Option<Applied>,
}

impl State {
    pub fn current_style(&self) -> StyleId {
        self.styles
            .style_for(self.current_app.as_ref().map(|a| a.bundle_id.as_str()))
    }

    pub fn set_frontmost(&mut self, app: AppRef) {
        if app.bundle_id.is_empty() {
            return;
        }
        self.recent.push(app.clone());
        self.current_app = Some(app);
    }
}

/// Starea comuna. Setarile se schimba din daemon si se aplica la urmatoarea dictare, fara repornire.
#[derive(Clone, Default)]
pub struct Shared(Arc<Mutex<State>>);

impl Shared {
    pub fn new(s: &Settings) -> Self {
        Self(Arc::new(Mutex::new(State {
            base_prompt: s.cleanup.prompt.clone(),
            level: s.cleanup.level,
            styles: s.styles.clone(),
            ..Default::default()
        })))
    }

    pub fn with<R>(&self, f: impl FnOnce(&mut State) -> R) -> R {
        f(&mut self.0.lock().unwrap_or_else(|e| e.into_inner()))
    }
}

/// Curatarea cu stil: poate fi pornita / oprita din meniu, sare peste model la stilul "Code"
/// si trimite modelului promptul asamblat pentru aplicatia din fata.
pub struct StyleCleaner {
    pub inner: vf_llm_openai::OpenAiCleaner,
    pub enabled: Arc<AtomicBool>,
    pub shared: Shared,
}

impl TextCleaner for StyleCleaner {
    fn clean(&self, text: &str, terms: &[&str]) -> anyhow::Result<Cleaned> {
        if !self.enabled.load(Ordering::Relaxed) {
            self.shared.with(|st| st.last_applied = None);
            return Ok(Cleaned::new(text));
        }
        // Promptul se asambleaza sub lacat, apelul la model (lent) se face fara el.
        let plan = self.shared.with(|st| {
            let app = st.current_app.clone();
            let plan = styles::plan(
                &st.base_prompt,
                st.level,
                &st.styles,
                app.as_ref().map(|a| a.bundle_id.as_str()),
                terms,
            );
            st.last_applied = Some(Applied {
                style: plan.style,
                level: plan.level,
                app,
                cleaned: plan.system_prompt.is_some(),
            });
            plan
        });
        let Some(prompt) = &plan.system_prompt else {
            return Ok(Cleaned::new(text));
        };
        Ok(Cleaned {
            text: self.inner.clean_with_prompt(prompt, text)?,
            final_period: plan.final_period,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cleaner(enabled: bool, shared: &Shared) -> StyleCleaner {
        // Adresa nu e folosita in teste: cazurile de aici nu ajung la model.
        let settings = vf_domain::CleanupSettings {
            url: "http://127.0.0.1:9/v1/chat/completions".into(),
            timeout_secs: 1,
            ..Default::default()
        };
        StyleCleaner {
            inner: vf_llm_openai::OpenAiCleaner::new(&settings),
            enabled: Arc::new(AtomicBool::new(enabled)),
            shared: shared.clone(),
        }
    }

    fn app(id: &str, name: &str) -> AppRef {
        AppRef {
            bundle_id: id.into(),
            name: name.into(),
        }
    }

    #[test]
    fn code_style_returns_text_untouched_without_calling_the_model() {
        let shared = Shared::default();
        shared.with(|st| st.set_frontmost(app("com.apple.Terminal", "Terminal")));
        let out = cleaner(true, &shared).clean("git status", &[]).unwrap();
        assert_eq!(out, Cleaned::new("git status"));
        let applied = shared.with(|st| st.last_applied.clone()).unwrap();
        assert_eq!(applied.style, StyleId::Code);
        assert!(!applied.cleaned);
        assert_eq!(applied.app.unwrap().name, "Terminal");
    }

    #[test]
    fn disabled_cleanup_applies_no_style() {
        let shared = Shared::default();
        shared.with(|st| {
            st.set_frontmost(app("com.tinyspeck.slackmacgap", "Slack"));
            st.last_applied = Some(Applied {
                style: StyleId::Email,
                level: CleanupLevel::High,
                app: None,
                cleaned: true,
            });
        });
        let out = cleaner(false, &shared).clean("salut", &[]).unwrap();
        assert_eq!(out, Cleaned::new("salut"));
        assert!(shared.with(|st| st.last_applied.clone()).is_none());
    }

    #[test]
    fn settings_changes_apply_to_the_next_dictation() {
        let shared = Shared::default();
        shared.with(|st| st.set_frontmost(app("com.apple.Terminal", "Terminal")));
        assert_eq!(shared.with(|st| st.current_style()), StyleId::Code);
        shared.with(|st| st.styles.enabled = false);
        assert_eq!(shared.with(|st| st.current_style()), StyleId::Default);
        // Cu stilurile oprite terminalul ajunge la model; fara server, apelul esueaza
        // (iar `Dictation` livreaza textul necuratat).
        assert!(cleaner(true, &shared).clean("git status", &[]).is_err());
        assert!(shared.with(|st| st.last_applied.clone()).unwrap().cleaned);
    }

    #[test]
    fn frontmost_app_feeds_the_recent_list() {
        let shared = Shared::default();
        shared.with(|st| {
            st.set_frontmost(app("com.apple.mail", "Mail"));
            st.set_frontmost(app("com.apple.Safari", "Safari"));
            st.set_frontmost(app("", "fara id"));
        });
        shared.with(|st| {
            assert_eq!(st.current_app.as_ref().unwrap().name, "Safari");
            assert_eq!(st.recent.list().len(), 2);
            assert_eq!(st.current_style(), StyleId::Default);
        });
    }
}
