use super::*;
use crate::Rules;

fn rules() -> MediafireRules {
    Rules::embedded().mediafire
}

fn t(s: &str) -> Option<Target> {
    target(&Url::parse(s).unwrap(), &rules().hosts)
}

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/mediafire/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path).unwrap()
}

fn page_url() -> Url {
    Url::parse("https://www.mediafire.com/file/q1w2e3r4t5y6u7i/Relatorio_Anual_2025.pdf/file")
        .unwrap()
}

#[test]
fn formatos_de_link() {
    let file = |k: &str| Some(Target::File(k.to_owned()));
    let folder = |k: &str| Some(Target::Folder(k.to_owned()));
    let k = "q1w2e3r4t5y6u7i";
    assert_eq!(
        t(&format!(
            "https://www.mediafire.com/file/{k}/Relatorio.pdf/file"
        )),
        file(k)
    );
    assert_eq!(
        t(&format!("https://www.mediafire.com/file/{k}/Relatorio.pdf")),
        file(k)
    );
    assert_eq!(t(&format!("https://www.mediafire.com/file/{k}")), file(k));
    assert_eq!(
        t(&format!(
            "https://mediafire.com/file/{k}/Relatorio.pdf/file"
        )),
        file(k)
    );
    assert_eq!(
        t(&format!(
            "http://www.mediafire.com/file/{k}/Relatorio.pdf/file"
        )),
        file(k)
    );
    assert_eq!(t(&format!("https://m.mediafire.com/file/{k}")), file(k));
    assert_eq!(t(&format!("https://www.mediafire.com/?{k}")), file(k));
    assert_eq!(
        t(&format!(
            "https://www.mediafire.com/download/{k}/Relatorio.pdf"
        )),
        file(k)
    );
    assert_eq!(
        t(&format!(
            "https://www.mediafire.com/view/{k}/Relatorio.pdf/file"
        )),
        file(k)
    );
    assert_eq!(
        t(&format!(
            "https://www.mediafire.com/file_premium/{k}/Relatorio.pdf/file"
        )),
        file(k)
    );
    assert_eq!(
        t("https://www.mediafire.com/file/Q1W2E3R4T5Y6U7I/a.pdf/file"),
        file(k)
    );
    assert_eq!(
        t("https://www.mediafire.com/folder/abc123folderk/Minha_Pasta"),
        folder("abc123folderk")
    );
    assert_eq!(
        t("https://www.mediafire.com/folder/abc123folderk"),
        folder("abc123folderk")
    );
    // não são arquivos do Mediafire
    assert_eq!(t("https://www.mediafire.com/"), None);
    assert_eq!(t("https://www.mediafire.com/?a=b"), None);
    assert_eq!(t("https://www.mediafire.com/help/"), None);
    assert_eq!(t("https://www.mediafire.com/file/x/a.pdf"), None);
    assert_eq!(
        t(&format!("https://mediafire.com.evil.example/file/{k}")),
        None
    );
}

#[test]
fn pagina_com_link_embaralhado() {
    let d = download_page(&page_url(), &fixture("page_ok.html"), &rules()).unwrap();
    assert_eq!(d.url.host_str(), Some("download2390.mediafire.com"));
    assert!(d.url.path().ends_with("Relatorio+Anual+2025.pdf"));
    assert_eq!(d.name.as_deref(), Some("Relatorio Anual 2025.pdf"));
    assert_eq!(d.size, Some(13_107_200));
}

#[test]
fn pagina_com_link_simples() {
    let d = download_page(&page_url(), &fixture("page_plain_href.html"), &rules()).unwrap();
    assert_eq!(d.url.host_str(), Some("download2390.mediafire.com"));
}

#[test]
fn captcha_e_arquivo_perigoso_pedem_o_navegador() {
    let captcha = download_page(&page_url(), &fixture("page_captcha.html"), &rules());
    assert!(matches!(captcha, Err(HostError::BrowserRequired(m)) if m.contains("captcha")));
    let danger = download_page(&page_url(), &fixture("page_dangerous.html"), &rules());
    assert!(matches!(danger, Err(HostError::BrowserRequired(m)) if m.contains("perigoso")));
    let password = download_page(&page_url(), &fixture("page_password.html"), &rules());
    assert!(matches!(password, Err(HostError::BrowserRequired(m)) if m.contains("senha")));
}

#[test]
fn removido_fica_offline() {
    let error_url =
        Url::parse("https://www.mediafire.com/error.php?errno=320&origin=download").unwrap();
    assert_eq!(
        download_page(&error_url, &fixture("page_removed.html"), &rules()),
        Err(HostError::Offline)
    );
    assert_eq!(
        file_meta(&fixture("get_info_invalid.json")),
        Err(HostError::Offline)
    );
}

#[test]
fn api_de_info_e_de_pasta() {
    let meta = file_meta(&fixture("get_info_ok.json")).unwrap();
    assert_eq!(meta.name, "Relatorio Anual 2025.pdf");
    assert_eq!(meta.size, Some(13_107_200));
    assert_eq!(meta.sha256.as_deref().map(str::len), Some(64));
    let c = folder_chunk(&fixture("folder_content.json")).unwrap();
    assert_eq!(c.files, vec!["aaa111bbb222c", "ddd333eee444f"]);
    assert!(c.folders.is_empty());
    assert!(!c.more);
    let c = folder_chunk(&fixture("folder_subfolders.json")).unwrap();
    assert!(c.files.is_empty());
    assert_eq!(c.folders, vec!["sub111folder1", "sub222folder2"]);
    assert!(c.more);
}

#[test]
fn seletor_trocado_no_override_muda_o_resultado() {
    // Portão (e): uma regra nova, sem recompilar, muda o que a página rende.
    let r = Rules::with_override("[mediafire]\ndownload_selector = \"a#outroBotao\"\n").unwrap();
    let res = download_page(&page_url(), &fixture("page_ok.html"), &r.mediafire);
    assert!(matches!(res, Err(HostError::Changed(_))));
}
