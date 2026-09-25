use super::*;

const HOSTS: &[&str] = &["pixeldrain.com", "pixeldrain.net", "pixeldra.in"];

fn hosts() -> Vec<String> {
    HOSTS.iter().map(|h| (*h).to_owned()).collect()
}

fn t(s: &str) -> Option<Target> {
    target(&Url::parse(s).unwrap(), &hosts())
}

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/pixeldrain/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path).unwrap()
}

#[test]
fn formatos_de_link() {
    let file = |id: &str| Some(Target::File(id.to_owned()));
    let list = |id: &str| Some(Target::List(id.to_owned()));
    assert_eq!(t("https://pixeldrain.com/u/Ab12CdEf"), file("Ab12CdEf"));
    assert_eq!(t("http://pixeldrain.com/u/Ab12CdEf"), file("Ab12CdEf"));
    assert_eq!(t("https://pixeldrain.com/u/Ab12CdEf/"), file("Ab12CdEf"));
    assert_eq!(
        t("https://pixeldrain.com/u/Ab12CdEf?download"),
        file("Ab12CdEf")
    );
    assert_eq!(t("https://www.pixeldrain.com/u/Ab12CdEf"), file("Ab12CdEf"));
    assert_eq!(t("https://pixeldrain.net/u/Ab12CdEf"), file("Ab12CdEf"));
    assert_eq!(t("https://pixeldra.in/u/Ab12CdEf"), file("Ab12CdEf"));
    assert_eq!(
        t("https://pixeldrain.com/api/file/Ab12CdEf"),
        file("Ab12CdEf")
    );
    assert_eq!(
        t("https://pixeldrain.com/api/file/Ab12CdEf?download"),
        file("Ab12CdEf")
    );
    assert_eq!(
        t("https://pixeldrain.com/api/file/Ab12CdEf/info"),
        file("Ab12CdEf")
    );
    assert_eq!(t("https://pixeldrain.com/l/Lst123Ab"), list("Lst123Ab"));
    assert_eq!(
        t("https://pixeldrain.com/l/Lst123Ab#item=2"),
        list("Lst123Ab")
    );
    assert_eq!(
        t("https://pixeldrain.com/api/list/Lst123Ab"),
        list("Lst123Ab")
    );
    // não são arquivos do Pixeldrain
    assert_eq!(t("https://pixeldrain.com/"), None);
    assert_eq!(t("https://pixeldrain.com/u/"), None);
    assert_eq!(t("https://pixeldrain.com/u/a%2Fb"), None);
    assert_eq!(t("https://notpixeldrain.com/u/Ab12CdEf"), None);
    assert_eq!(t("https://pixeldrain.com/about"), None);
}

#[test]
fn info_de_arquivo_liberado() {
    let info = info(200, &fixture("info_ok.json")).unwrap();
    assert_eq!(info.name, "ubuntu-24.04.1-desktop-amd64.iso");
    assert_eq!(info.size, 6_203_355_136);
    assert_eq!(info.hash_sha256.as_deref().map(str::len), Some(64));
    assert_eq!(check_available(&info), Ok(()));
}

#[test]
fn captcha_vira_pedido_de_navegador_sem_contornar() {
    let info = info(200, &fixture("info_captcha.json")).unwrap();
    let err = check_available(&info).unwrap_err();
    assert!(matches!(&err, HostError::BrowserRequired(m) if m.contains("too much bandwidth")));
}

#[test]
fn arquivo_removido_fica_offline() {
    assert_eq!(
        info(404, &fixture("info_not_found.json")),
        Err(HostError::Offline)
    );
    assert_eq!(info(500, "oops"), Err(HostError::Http(500)));
}

#[test]
fn lista_vira_arquivos() {
    let ids = list_files(200, &fixture("list_ok.json")).unwrap();
    assert_eq!(ids, vec!["Aa11Bb22", "Cc33Dd44", "Ee55Ff66"]);
    assert_eq!(
        list_files(404, &fixture("info_not_found.json")),
        Err(HostError::Offline)
    );
}
