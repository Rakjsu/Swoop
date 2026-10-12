//! Formulários HTML das páginas dos servidores: campos a enviar e destino.

use crate::page;
use scraper::{ElementRef, Html};
use swoop_core::HostError;
use url::Url;

/// Formulário encontrado numa página: campos a enviar e destino.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Form {
    pub fields: Vec<(String, String)>,
    /// `action` do formulário; vazio = a própria página.
    pub action: Option<String>,
}

impl Form {
    /// Destino absoluto do formulário.
    pub fn target(&self, page: &Url) -> Url {
        self.action
            .as_deref()
            .filter(|a| !a.trim().is_empty())
            .and_then(|a| page.join(a).ok())
            .unwrap_or_else(|| page.clone())
    }

    /// Lê um `<form>`: os `<input name>` que não são botão (`keep` decide
    /// os botões que também contam, como os `method_*` do XFileSharing).
    pub fn read(form: ElementRef<'_>, keep: impl Fn(&ElementRef<'_>) -> bool) -> Self {
        let inputs = page::selector("input[name]").expect("seletor fixo");
        let fields = form
            .select(&inputs)
            .filter(|i| {
                !matches!(i.value().attr("type"), Some("submit" | "image" | "button")) || keep(i)
            })
            .filter_map(|i| {
                Some((
                    i.value().attr("name")?.to_owned(),
                    i.value().attr("value").unwrap_or("").to_owned(),
                ))
            })
            .collect();
        Self {
            fields,
            action: form.value().attr("action").map(str::to_owned),
        }
    }

    /// Primeiro formulário que casa com o seletor das regras.
    pub fn by_selector(doc: &Html, css: &str) -> Result<Option<Self>, HostError> {
        let sel = page::selector(css)?;
        Ok(doc.select(&sel).next().map(|f| Self::read(f, |_| false)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_campos_e_destino() {
        let doc = Html::parse_document(
            "<form id='f' action='/enviar'><input type='hidden' name='csrf' value='x1'>\
             <input name='code'><input type='submit' name='ok' value='Enviar'></form>",
        );
        let form = Form::by_selector(&doc, "form#f").unwrap().unwrap();
        assert_eq!(
            form.fields,
            vec![("csrf".into(), "x1".into()), ("code".into(), String::new())]
        );
        let page = Url::parse("https://site.exemplo/a/b").unwrap();
        assert_eq!(form.target(&page).as_str(), "https://site.exemplo/enviar");
        assert!(Form::by_selector(&doc, "form#outro").unwrap().is_none());
    }
}
