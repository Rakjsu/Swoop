//! Pedidos que a interface faz ao motor e as respostas.
//!
//! Ids são `i64` puros (o mesmo número da tabela `downloads`), para o JSON
//! ficar simples do lado TypeScript.

use serde::{Deserialize, Serialize};
use swoop_core::Settings;
use ts_rs::TS;

/// Pedido da interface. No JSON: `{ "type": "pause", "id": 3 }`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Command {
    /// Adiciona links num pacote novo; `dest` vazio = pasta padrão.
    AddLinks {
        links: Vec<String>,
        #[ts(optional)]
        dest: Option<String>,
    },
    Pause {
        id: i64,
    },
    Resume {
        id: i64,
    },
    /// Nova tentativa de um download que falhou.
    Retry {
        id: i64,
    },
    /// Tira da lista. O `.part` de um download incompleto sempre é apagado;
    /// o arquivo final só com `delete_file`.
    Remove {
        id: i64,
        #[serde(default)]
        delete_file: bool,
    },
    PauseAll,
    ResumeAll,
    /// Limite global em bytes/s; ausente = sem limite. Fica salvo.
    SetSpeedLimit {
        #[ts(optional)]
        bps: Option<u64>,
    },
    /// Grava as preferências e aplica na hora (limite, simultâneos…); as de
    /// conexões valem para os próximos downloads iniciados.
    SaveSettings {
        settings: Settings,
    },
    /// Coletor: acha os links do texto (pastas viram arquivos) e confere.
    Collect {
        text: String,
    },
    /// Coletor → fila, num pacote; `dest` vazio = pasta automática.
    CollectorStart {
        ids: Vec<i64>,
        #[ts(optional)]
        dest: Option<String>,
    },
    /// Tira links do coletor.
    CollectorRemove {
        ids: Vec<i64>,
    },
    /// Tira do coletor os links offline.
    CollectorRemoveOffline,
    /// Esvazia o coletor.
    CollectorClear,
}

/// Resposta a um `Command`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum Reply {
    Done,
    /// Ids dos downloads criados por `AddLinks`.
    Added {
        ids: Vec<i64>,
    },
    /// Links novos que entraram no coletor (repetidos não contam).
    Collected {
        added: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn comando_le_o_json_da_interface() {
        let cmd: Command = serde_json::from_value(json!({ "type": "pause", "id": 7 })).unwrap();
        assert_eq!(cmd, Command::Pause { id: 7 });

        let cmd: Command =
            serde_json::from_value(json!({ "type": "add_links", "links": ["http://a/b"] }))
                .unwrap();
        assert_eq!(
            cmd,
            Command::AddLinks {
                links: vec!["http://a/b".into()],
                dest: None
            }
        );

        let cmd: Command =
            serde_json::from_value(json!({ "type": "collector_start", "ids": [3] })).unwrap();
        assert_eq!(
            cmd,
            Command::CollectorStart {
                ids: vec![3],
                dest: None
            }
        );

        let cmd: Command = serde_json::from_value(json!({ "type": "remove", "id": 2 })).unwrap();
        assert_eq!(
            cmd,
            Command::Remove {
                id: 2,
                delete_file: false
            }
        );
    }

    #[test]
    fn resposta_vira_json_com_tipo() {
        let json = serde_json::to_value(Reply::Added { ids: vec![1, 2] }).unwrap();
        assert_eq!(json, json!({ "type": "added", "ids": [1, 2] }));
    }
}
