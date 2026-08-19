//! タブの振り分けロジック。
//!
//! タブエリアは次の 3 つに分かれる。
//!
//! * 左端に固定される「ターミナル」タブ（ドロップダウン）
//! * その右に固定される「プロジェクト」タブ（ドロップダウン。開いているプロジェクトのシート）
//! * それ以外の通常のシートタブ（ドラッグで並べ替え可能）
//!
//! どのシートがどこに入るかはインデックス演算だけで決まるため、
//! 純粋関数として切り出して単体テストの対象にしている。

use std::collections::HashSet;

/// ターミナルタブの ID 接頭辞
pub const TERMINAL_ID_PREFIX: &str = "__TERM__";

pub fn is_terminal_tab(id: &str) -> bool {
    id.starts_with(TERMINAL_ID_PREFIX)
}

/// Drive 上のファイル名（`{guid}.{拡張子}`）から guid を取り出す
pub fn guid_from_drive_name(name: &str) -> String {
    match name.rfind('.') {
        Some(pos) if pos > 0 => name[..pos].to_string(),
        _ => name.to_string(),
    }
}

/// シートがどのプロジェクト guid に対応するかの候補を返す。
///
/// `Sheet.guid` は開かれた経路によって拡張子が残る場合（`abc.md`）と
/// 残らない場合（`abc`）があるため、`guid` と `title` の双方から
/// 拡張子を除いた値も候補に含めて取りこぼしを防ぐ。
pub fn sheet_project_keys(guid: Option<&str>, title: &str) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(guid) = guid {
        keys.push(guid.to_string());
        keys.push(guid_from_drive_name(guid));
    }
    if !title.is_empty() {
        keys.push(guid_from_drive_name(title));
    }
    keys
}

/// 重複タブ整理の入力となるタブ 1 件
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DedupeEntry {
    pub id: String,
    /// プロジェクト guid 候補（`sheet_project_keys` の結果）
    pub keys: Vec<String>,
    pub drive_id: Option<String>,
    /// 未保存の変更を抱えているか
    pub has_unsaved: bool,
}

/// 同じ Drive 上のファイルを指すかどうかの識別子。
/// 判定できない（未保存の新規シート等）場合は None を返し、重複扱いしない。
fn identity_of(entry: &DedupeEntry) -> Option<String> {
    if let Some(drive_id) = entry.drive_id.as_ref() {
        return Some(format!("drive:{}", drive_id));
    }
    // guid は拡張子の有無で表記が揺れるため、除いた形で揃える
    entry
        .keys
        .iter()
        .map(|k| guid_from_drive_name(k))
        .find(|k| !k.is_empty())
        .map(|k| format!("guid:{}", k))
}

/// 同じシートが複数のタブとして開かれている場合に、閉じてよいタブの ID を返す。
///
/// * 最初の 1 つは必ず残す
/// * 未保存の変更を抱えているタブは、重複していても残す（編集内容を失わないため）
/// * Drive 上に実体が無いシート（未保存の新規シート）は重複扱いしない
pub fn duplicate_tab_ids(entries: &[DedupeEntry]) -> Vec<String> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut removable = Vec::new();
    for entry in entries {
        let identity = match identity_of(entry) {
            Some(id) => id,
            None => continue,
        };
        if seen.insert(identity) {
            continue; // 最初の 1 つ
        }
        if !entry.has_unsaved {
            removable.push(entry.id.clone());
        }
    }
    removable
}

#[cfg(test)]
mod tests {
    use super::*;


    fn dup(id: &str, guid: Option<&str>, title: &str, drive_id: Option<&str>, unsaved: bool) -> DedupeEntry {
        DedupeEntry {
            id: id.to_string(),
            keys: sheet_project_keys(guid, title),
            drive_id: drive_id.map(|d| d.to_string()),
            has_unsaved: unsaved,
        }
    }

    #[test]
    fn no_duplicates_means_nothing_to_remove() {
        let entries = vec![
            dup("s1", Some("guid-a"), "guid-a.txt", Some("drive-a"), false),
            dup("s2", Some("guid-b"), "guid-b.txt", Some("drive-b"), false),
        ];
        assert!(duplicate_tab_ids(&entries).is_empty());
    }

    #[test]
    fn later_tabs_of_the_same_drive_file_are_removable() {
        let entries = vec![
            dup("s1", Some("guid-a"), "guid-a.txt", Some("drive-a"), false),
            dup("s2", Some("guid-a"), "guid-a.txt", Some("drive-a"), false),
            dup("s3", Some("guid-a"), "guid-a.txt", Some("drive-a"), false),
        ];
        // 最初の 1 つは残す
        assert_eq!(duplicate_tab_ids(&entries), vec!["s2", "s3"]);
    }

    #[test]
    fn duplicates_match_across_guid_extension_forms() {
        // guid に拡張子が残っている経路と残らない経路で開かれた同じシート
        let entries = vec![
            dup("s1", Some("guid-a"), "guid-a.md", None, false),
            dup("s2", Some("guid-a.md"), "guid-a.md", None, false),
        ];
        assert_eq!(duplicate_tab_ids(&entries), vec!["s2"]);
    }

    #[test]
    fn unsaved_duplicates_are_kept() {
        let entries = vec![
            dup("s1", Some("guid-a"), "guid-a.txt", Some("drive-a"), false),
            dup("s2", Some("guid-a"), "guid-a.txt", Some("drive-a"), true),
        ];
        // 編集中の内容を失わないよう、未保存のタブは重複でも残す
        assert!(duplicate_tab_ids(&entries).is_empty());
    }

    #[test]
    fn unsaved_new_sheets_are_never_treated_as_duplicates() {
        // Drive 上に実体が無い新規シートは、何枚あっても重複ではない
        let entries = vec![
            dup("s1", None, "", None, false),
            dup("s2", None, "", None, false),
        ];
        assert!(duplicate_tab_ids(&entries).is_empty());
    }

    #[test]
    fn drive_id_takes_precedence_over_guid() {
        // 同じ guid でも別ファイル（drive_id が違う）なら重複ではない
        let entries = vec![
            dup("s1", Some("guid-a"), "guid-a.txt", Some("drive-a"), false),
            dup("s2", Some("guid-a"), "guid-a.txt", Some("drive-b"), false),
        ];
        assert!(duplicate_tab_ids(&entries).is_empty());
    }

    #[test]
    fn terminal_ids_are_detected_by_prefix() {
        assert!(is_terminal_tab("__TERM__1"));
        assert!(!is_terminal_tab("1750000000000"));
        assert!(!is_terminal_tab(""));
    }

    #[test]
    fn guid_is_extracted_from_drive_file_name() {
        assert_eq!(guid_from_drive_name("abc-123.txt"), "abc-123");
        assert_eq!(guid_from_drive_name("abc-123.md"), "abc-123");
        // 拡張子が無い場合はそのまま
        assert_eq!(guid_from_drive_name("abc-123"), "abc-123");
        // 先頭がドットのファイルは削らない
        assert_eq!(guid_from_drive_name(".leaf_projects"), ".leaf_projects");
    }

    #[test]
    fn project_keys_cover_both_guid_forms() {
        // guid に拡張子が残っている経路でも、除いた形が候補に入る
        let keys = sheet_project_keys(Some("abc-123.md"), "abc-123.md");
        assert!(keys.contains(&"abc-123.md".to_string()));
        assert!(keys.contains(&"abc-123".to_string()));

        // guid が無くてもファイル名から拾える
        let keys = sheet_project_keys(None, "abc-123.txt");
        assert_eq!(keys, vec!["abc-123".to_string()]);

        // どちらも無ければ空
        assert!(sheet_project_keys(None, "").is_empty());
    }

}
