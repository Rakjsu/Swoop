use super::*;

fn setup() -> Connection {
    let mut c = Connection::open_in_memory().unwrap();
    crate::migrations::run(&mut c).unwrap();
    c
}

fn states(c: &Connection) -> Vec<(String, LinkState)> {
    list(c)
        .unwrap()
        .into_iter()
        .map(|r| (r.url, r.state))
        .collect()
}

#[test]
fn mesma_url_nao_entra_duas_vezes() {
    let c = setup();
    let b = next_batch(&c).unwrap();
    assert!(insert(&c, "http://a/1", "a", b).unwrap());
    assert!(!insert(&c, "http://a/1", "a", b).unwrap());
    assert_eq!(list(&c).unwrap().len(), 1);
    assert_eq!(next_batch(&c).unwrap(), b + 1);
}

#[test]
fn verifica_no_maximo_um_por_servidor() {
    let c = setup();
    for (url, host) in [
        ("http://a/1", "a"),
        ("http://a/2", "a"),
        ("http://b/1", "b"),
        ("http://c/1", "c"),
    ] {
        insert(&c, url, host, 1).unwrap();
    }
    let first: Vec<String> = claim(&c, 2).unwrap().into_iter().map(|r| r.url).collect();
    assert_eq!(first, ["http://a/1", "http://b/1"]);
    // "a" e "b" ocupados: só "c" sai agora
    let second: Vec<String> = claim(&c, 4).unwrap().into_iter().map(|r| r.url).collect();
    assert_eq!(second, ["http://c/1"]);
    set_result(&c, 1, LinkState::Online, Some("1.bin"), Some(10), None).unwrap();
    let third: Vec<String> = claim(&c, 4).unwrap().into_iter().map(|r| r.url).collect();
    assert_eq!(third, ["http://a/2"]);
}

#[test]
fn iniciar_leva_so_o_que_pode_baixar() {
    let c = setup();
    for url in ["http://a/1", "http://a/2", "http://a/3", "http://a/4"] {
        insert(&c, url, "a", 1).unwrap();
    }
    set_result(&c, 1, LinkState::Online, Some("1.bin"), Some(10), None).unwrap();
    set_result(&c, 2, LinkState::Offline, None, None, Some("removido")).unwrap();
    set_result(&c, 3, LinkState::Failed, None, None, Some("rede")).unwrap();
    assert!(batch_pending(&c, 1).unwrap());
    assert_eq!(online_in_batch(&c, 1).unwrap(), vec![1]);

    let taken: Vec<i64> = take(&c, &[3, 1, 2])
        .unwrap()
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(taken, vec![1, 3]);
    assert_eq!(
        states(&c),
        vec![
            ("http://a/2".into(), LinkState::Offline),
            ("http://a/4".into(), LinkState::Unchecked)
        ]
    );
    assert_eq!(remove_offline(&c).unwrap(), 1);
    set_result(&c, 4, LinkState::Online, Some("4.bin"), None, None).unwrap();
    assert!(!batch_pending(&c, 1).unwrap());
    assert_eq!(clear(&c).unwrap(), 1);
}

#[test]
fn verificacao_interrompida_volta_para_a_fila() {
    let c = setup();
    insert(&c, "http://a/1", "a", 1).unwrap();
    claim(&c, 1).unwrap();
    assert_eq!(recover(&c).unwrap(), 1);
    assert_eq!(
        states(&c),
        vec![("http://a/1".into(), LinkState::Unchecked)]
    );
    assert_eq!(remove(&c, &[1, 99]).unwrap(), 1);
}
