//! Alt+Shift+R（アクティブなシートの再読み込み）の判定。
//!
//! 「どこから読み直せるか」と「読み直した本文をどう扱うか」を、表示や通信から
//! 切り離して単体テストの対象にしている。
//! 仕様は docs/SHEET_RELOAD_AND_SWITCHER_BADGE_SPEC.md §1。

/// ローカルファイルのシートを表すカテゴリ
const LOCAL_CATEGORY: &str = "__LOCAL__";

/// 再読み込みの対象（アクティブなシートの状態）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReloadTarget<'a> {
    /// ターミナルがアクティブか
    pub terminal_active: bool,
    /// アクティブなシートの Drive ID
    pub drive_id: Option<&'a str>,
    /// アクティブなシートのカテゴリ（`__LOCAL__` はローカルファイル）
    pub category: &'a str,
    /// ローカルファイルの絶対パス（デスクトップ版で開いた時だけ入る。再起動すると消える）
    pub local_path: Option<&'a str>,
    /// シートのタイトル（ローカルファイルではファイル名）
    pub title: &'a str,
    /// Web 版で保持しているファイルハンドルのファイル名（無ければ None）。
    /// ブラウザは最後に開いた／保存したローカルファイルのハンドルを 1 つだけ持つ
    pub web_handle_name: Option<&'a str>,
}

/// どこから読み直すか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadSource {
    /// 読み直せない（何もしない）
    None,
    /// Google ドライブから
    Drive,
    /// ローカルファイルをパスから（デスクトップ版）
    LocalPath,
    /// ローカルファイルを保持しているファイルハンドルから（Web 版）
    LocalHandle,
}

/// アクティブなシートをどこから読み直すかを決める。
///
/// * ターミナルがアクティブなら None
/// * ローカルファイルのシート（カテゴリ `__LOCAL__` かパスあり）は、
///   パスがあれば LocalPath、Web 版のハンドルのファイル名がタイトルと一致すれば LocalHandle、
///   どちらも無ければ None
/// * Drive ID があれば Drive
/// * それ以外（未保存の新規シート）は None
pub fn reload_source(target: &ReloadTarget) -> ReloadSource {
    if target.terminal_active {
        return ReloadSource::None;
    }
    let is_local = target.category == LOCAL_CATEGORY || target.local_path.is_some();
    if is_local {
        // ローカルファイルは Drive からは読まない
        if target.local_path.is_some() {
            return ReloadSource::LocalPath;
        }
        if target.web_handle_name == Some(target.title) {
            return ReloadSource::LocalHandle;
        }
        return ReloadSource::None;
    }
    if target.drive_id.is_some() {
        ReloadSource::Drive
    } else {
        ReloadSource::None
    }
}

/// 再読み込みで取得した本文をどう扱うか
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadDecision {
    /// 今の本文と同じ。何もしない（同期時刻だけ追従する）
    NoChange,
    /// 取得した本文で置き換える
    Replace,
    /// 未保存の編集があるので、作業者に選ばせる
    Conflict,
}

/// 再読み込み（Alt+Shift+R）で、取得した本文をどう扱うかを決める。
///
/// バックグラウンドの同期チェックは更新時刻とファイルサイズで推測するが
/// （同じサイズなら名前変更などメタデータだけの変更とみなす）、
/// 1 文字の書き換えのようにサイズが変わらない編集を取りこぼす。
/// 手動の時は本文そのものを比べる。Drive とローカルファイルで共通。
///
/// * 本文が同じなら NoChange
/// * 違っていて、未保存の編集が無ければ Replace
/// * 違っていて、未保存の編集があれば Conflict
pub fn decide_reload(local: &str, fetched: &str, has_local_edit: bool) -> ReloadDecision {
    if local == fetched {
        ReloadDecision::NoChange
    } else if has_local_edit {
        ReloadDecision::Conflict
    } else {
        ReloadDecision::Replace
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive_sheet() -> ReloadTarget<'static> {
        ReloadTarget {
            terminal_active: false,
            drive_id: Some("drive-1"),
            category: "cat-1",
            local_path: None,
            title: "abc.md",
            web_handle_name: None,
        }
    }

    fn local_sheet() -> ReloadTarget<'static> {
        ReloadTarget {
            terminal_active: false,
            drive_id: None,
            category: "__LOCAL__",
            local_path: None,
            title: "notes.txt",
            web_handle_name: None,
        }
    }

    // ---- どこから読み直すか ----

    #[test]
    fn drive_sheet_reloads_from_drive() {
        assert_eq!(reload_source(&drive_sheet()), ReloadSource::Drive);
        // カテゴリ無し（「カテゴリなし」フォルダ）でも Drive のシートなら Drive
        assert_eq!(reload_source(&ReloadTarget { category: "", ..drive_sheet() }), ReloadSource::Drive);
    }

    #[test]
    fn terminal_active_cannot_reload() {
        assert_eq!(reload_source(&ReloadTarget { terminal_active: true, ..drive_sheet() }), ReloadSource::None);
        let local = ReloadTarget { terminal_active: true, local_path: Some("/tmp/notes.txt"), ..local_sheet() };
        assert_eq!(reload_source(&local), ReloadSource::None);
    }

    #[test]
    fn unsaved_sheet_without_drive_id_cannot_reload() {
        let unsaved = ReloadTarget { drive_id: None, category: "cat-1", ..drive_sheet() };
        assert_eq!(reload_source(&unsaved), ReloadSource::None);
    }

    #[test]
    fn local_file_with_path_reloads_from_path() {
        let t = ReloadTarget { local_path: Some("/Users/me/notes.txt"), ..local_sheet() };
        assert_eq!(reload_source(&t), ReloadSource::LocalPath);
        // カテゴリが __LOCAL__ でなくてもパスを持っていればローカルファイル
        let t2 = ReloadTarget { category: "", local_path: Some("/Users/me/notes.txt"), ..local_sheet() };
        assert_eq!(reload_source(&t2), ReloadSource::LocalPath);
    }

    #[test]
    fn local_file_reloads_from_matching_web_handle() {
        let t = ReloadTarget { web_handle_name: Some("notes.txt"), ..local_sheet() };
        assert_eq!(reload_source(&t), ReloadSource::LocalHandle);
    }

    #[test]
    fn local_file_with_other_web_handle_cannot_reload() {
        // ブラウザが持っているのは別のファイルのハンドル
        let t = ReloadTarget { web_handle_name: Some("other.txt"), ..local_sheet() };
        assert_eq!(reload_source(&t), ReloadSource::None);
    }

    #[test]
    fn local_file_without_path_or_handle_cannot_reload() {
        // 再起動して復元されたローカルシートなど
        assert_eq!(reload_source(&local_sheet()), ReloadSource::None);
    }

    #[test]
    fn local_file_never_reloads_from_drive() {
        // 万一 drive_id が入っていてもローカルファイルは Drive から読まない
        let t = ReloadTarget { drive_id: Some("drive-1"), ..local_sheet() };
        assert_eq!(reload_source(&t), ReloadSource::None);
    }

    // ---- 読み直した本文をどう扱うか ----

    #[test]
    fn same_content_needs_no_change() {
        assert_eq!(decide_reload("abc", "abc", false), ReloadDecision::NoChange);
        // 未保存の編集フラグが立っていても、内容が同じなら何もしない
        assert_eq!(decide_reload("abc", "abc", true), ReloadDecision::NoChange);
    }

    #[test]
    fn different_content_without_local_edit_is_replaced() {
        assert_eq!(decide_reload("old", "new", false), ReloadDecision::Replace);
    }

    #[test]
    fn same_size_edit_is_still_detected() {
        // バイト数が変わらない書き換えも取りこぼさない（バックグラウンド同期の弱点）
        assert_eq!(decide_reload("abc", "abd", false), ReloadDecision::Replace);
    }

    #[test]
    fn different_content_with_local_edit_goes_to_conflict() {
        assert_eq!(decide_reload("local edit", "remote edit", true), ReloadDecision::Conflict);
    }
}
