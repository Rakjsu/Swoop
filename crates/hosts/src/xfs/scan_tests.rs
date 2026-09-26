use super::*;
use crate::Rules;

fn rules() -> XfsRules {
    Rules::embedded().xfs
}

fn count(html: &str) -> (u64, bool) {
    countdown(&Html::parse_document(html), &rules()).unwrap()
}

fn link(html: &str) -> Option<Url> {
    let page = Url::parse("https://fastfile.cc/abcdefgh1234").unwrap();
    final_link(&Html::parse_document(html), &page, &rules()).unwrap()
}

#[test]
fn contador_em_varios_formatos() {
    assert_eq!(count("<span class='seconds'>30</span>"), (30, true));
    assert_eq!(
        count("<div id='countdown'>Wait 60 seconds</div>"),
        (60, true)
    );
    assert_eq!(count("<span data-seconds='45'></span>"), (45, true));
    assert_eq!(
        count("<script>var countdown = 60; setInterval(tick, 1000);</script>"),
        (60, true)
    );
    assert_eq!(count("<script>timer: '75'</script>"), (75, true));
    // teto das regras
    assert_eq!(count("<span class='seconds'>99999</span>"), (600, true));
}

#[test]
fn sem_contador_legivel_vale_a_espera_padrao() {
    let fallback = rules().fallback_countdown_secs;
    assert_eq!(fallback, 60);
    assert_eq!(
        count("<form><input name='op' value='download2'></form>"),
        (60, false)
    );
    // um "count = 0" de anúncio não zera a espera
    assert_eq!(count("<script>ads.count = 0;</script>"), (60, false));
    // zero escrito no próprio contador vale (site sem espera)
    assert_eq!(count("<span class='seconds'>0</span>"), (0, true));
}

#[test]
fn link_final_por_refresh_script_ou_servidor_de_arquivos() {
    let meta =
        link("<meta http-equiv='Refresh' content='3; URL=https://s3.fastfile.cc/d/xyz/a.rar'>");
    assert_eq!(meta.unwrap().as_str(), "https://s3.fastfile.cc/d/xyz/a.rar");
    let relative = link("<meta http-equiv='refresh' content=\"0;url=/load/a.rar\">");
    assert_eq!(relative.unwrap().as_str(), "https://fastfile.cc/load/a.rar");
    let script =
        link("<script>window.location.href = \"https://s9.fastfile.cc/d/q/b.zip\";</script>");
    assert_eq!(script.unwrap().host_str(), Some("s9.fastfile.cc"));
    let anchor = link(
        "<div class='novo-tema'><a href='https://fastfile.cc/premium.html'>Premium</a>\
         <a href='https://s12.fastfile.cc:183/d/xyz/Relatorio%20Anual.pdf'>Baixar</a></div>",
    );
    assert_eq!(anchor.unwrap().port(), Some(183));
}

#[test]
fn links_que_nao_sao_o_arquivo_ficam_de_fora() {
    assert_eq!(
        link(
            "<a href='https://www.fastfile.cc/premium.html'>Premium</a>\
             <a href='https://help.fastfile.cc/faq'>Ajuda</a>\
             <a href='https://anuncio.example/promo.exe'>Oferta</a>\
             <a href='https://fastfile.cc.evil.example/d/x/a.rar'>x</a>"
        ),
        None
    );
    assert_eq!(
        link("<script>location.href = 'javascript:void(0)'</script>"),
        None
    );
}
