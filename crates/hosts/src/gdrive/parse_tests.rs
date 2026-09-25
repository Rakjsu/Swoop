use super::*;
use crate::Rules;

fn rules() -> GdriveRules {
    Rules::embedded().gdrive
}

fn t(s: &str) -> Option<Target> {
    target(&Url::parse(s).unwrap(), &rules().hosts)
}

fn fixture(name: &str) -> String {
    let path = format!(
        "{}/tests/fixtures/gdrive/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(path).unwrap()
}

const ID: &str = "1AbCdEfGhIjKlMnOpQrStUvWxYz012345";

fn download() -> Url {
    Url::parse(&format!(
        "https://drive.usercontent.google.com/download?id={ID}&export=download"
    ))
    .unwrap()
}

fn page(status: u16, final_url: &Url, body: &str) -> Result<Outcome, HostError> {
    let r = Response {
        status,
        final_url,
        file: false,
        name: None,
        size: None,
        body,
    };
    outcome(&r, &rules(), SystemTime::UNIX_EPOCH)
}

#[test]
fn formatos_de_link() {
    let file = || Some(Target::File(ID.to_owned()));
    let folder = || Some(Target::Folder(ID.to_owned()));
    assert_eq!(
        t(&format!(
            "https://drive.google.com/file/d/{ID}/view?usp=sharing"
        )),
        file()
    );
    assert_eq!(
        t(&format!("https://drive.google.com/file/d/{ID}/view")),
        file()
    );
    assert_eq!(t(&format!("https://drive.google.com/file/d/{ID}")), file());
    assert_eq!(
        t(&format!("https://drive.google.com/file/d/{ID}/edit")),
        file()
    );
    assert_eq!(
        t(&format!("https://drive.google.com/file/u/0/d/{ID}/view")),
        file()
    );
    assert_eq!(t(&format!("https://drive.google.com/open?id={ID}")), file());
    assert_eq!(
        t(&format!(
            "https://drive.google.com/uc?id={ID}&export=download"
        )),
        file()
    );
    assert_eq!(
        t(&format!(
            "https://drive.google.com/uc?export=download&id={ID}"
        )),
        file()
    );
    assert_eq!(
        t(&format!(
            "https://docs.google.com/uc?export=download&id={ID}"
        )),
        file()
    );
    assert_eq!(t(download().as_ref()), file());
    assert_eq!(
        t(&format!(
            "https://drive.google.com/drive/folders/{ID}?usp=sharing"
        )),
        folder()
    );
    assert_eq!(
        t(&format!("https://drive.google.com/drive/u/1/folders/{ID}")),
        folder()
    );
    assert_eq!(
        t(&format!("https://drive.google.com/folderview?id={ID}")),
        folder()
    );
    assert_eq!(
        t(&format!(
            "https://drive.google.com/embeddedfolderview?id={ID}#list"
        )),
        folder()
    );
    // não são arquivos do Drive (ou id inválido)
    assert_eq!(t("https://drive.google.com/"), None);
    assert_eq!(t("https://drive.google.com/drive/my-drive"), None);
    assert_eq!(t("https://drive.google.com/file/d/curto/view"), None);
    assert_eq!(t("https://drive.google.com/open?usp=sharing"), None);
    assert_eq!(
        t(&format!("https://docs.google.com/document/d/{ID}/edit")),
        None
    );
    assert_eq!(
        t(&format!(
            "https://drive.google.com.evil.example/file/d/{ID}/view"
        )),
        None
    );
}

#[test]
fn anexo_e_o_proprio_arquivo() {
    let url = download();
    let r = Response {
        status: 206,
        final_url: &url,
        file: true,
        name: Some("video.mp4".into()),
        size: Some(52_428_800),
        body: "",
    };
    assert_eq!(
        outcome(&r, &rules(), SystemTime::UNIX_EPOCH),
        Ok(Outcome::File {
            name: Some("video.mp4".into()),
            size: Some(52_428_800),
        })
    );
}

#[test]
fn confirmacao_leva_ao_formulario() {
    let Ok(Outcome::Confirm { url, name, size }) = page(200, &download(), &fixture("confirm.html"))
    else {
        panic!("esperava a confirmação");
    };
    assert_eq!(url.host_str(), Some("drive.usercontent.google.com"));
    assert_eq!(url.path(), "/download");
    let q: Vec<(String, String)> = url.query_pairs().into_owned().collect();
    assert!(q.contains(&("id".into(), ID.into())));
    assert!(q.contains(&("confirm".into(), "t".into())));
    assert!(q.iter().any(|(k, v)| k == "uuid" && !v.is_empty()));
    assert_eq!(name.as_deref(), Some("Backup Fotos 2024.zip"));
    assert_eq!(size, Some(1_288_490_189));
}

#[test]
fn muitos_usuarios_vira_espera_de_cota() {
    let res = page(200, &download(), &fixture("quota.html"));
    let until = SystemTime::UNIX_EPOCH + Duration::from_secs(60 * 60);
    assert_eq!(
        res,
        Err(HostError::Wait {
            until,
            reason: WaitReason::Quota,
        })
    );
}

#[test]
fn removido_privado_e_desconhecido() {
    assert_eq!(
        page(404, &download(), &fixture("missing.html")),
        Err(HostError::Offline)
    );
    let login = Url::parse("https://accounts.google.com/v3/signin/identifier?continue=x").unwrap();
    assert_eq!(
        page(200, &login, "<html>Sign in</html>"),
        Err(HostError::AccessDenied)
    );
    assert_eq!(
        page(403, &download(), "<html>forbidden</html>"),
        Err(HostError::AccessDenied)
    );
    assert!(matches!(
        page(200, &download(), "<html>nada</html>"),
        Err(HostError::Changed(_))
    ));
}

#[test]
fn pasta_lista_arquivos_e_subpastas_sem_repetir() {
    let entries = folder_entries(&fixture("folder.html"), &rules()).unwrap();
    assert_eq!(
        entries,
        vec![
            Target::File("1FiLeAaAaAaAaAaAaAaAaAaAaAaAaAa01".into()),
            Target::File("1FiLeBbBbBbBbBbBbBbBbBbBbBbBbBb02".into()),
            Target::Folder("1FoLdErCcCcCcCcCcCcCcCcCcCcCcCc03".into()),
        ]
    );
}
