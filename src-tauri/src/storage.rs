//! SQLite 保存任务原始记录和当前动作。修订校验与内容更新在同一事务内完成。
use crate::document::{TaskDocument, TaskSummary};
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;

pub use crate::capture::Hotkeys;
pub struct Store {
    connection: Connection,
}
impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        let connection =
            Connection::open(path).map_err(|e| format!("无法打开本地任务文件：{e}"))?;
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .map_err(db_error)?;
        if version > 1 {
            return Err("本地任务由较新版本创建，请使用更新的 Repeat 打开；原文件未修改".into());
        }
        connection.execute_batch("PRAGMA busy_timeout=5000; PRAGMA foreign_keys=ON; PRAGMA user_version=1; CREATE TABLE IF NOT EXISTS tasks (id TEXT PRIMARY KEY, name TEXT NOT NULL, revision INTEGER NOT NULL, document TEXT NOT NULL); CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);").map_err(|e| format!("无法初始化本地任务：{e}"))?;
        Ok(Self { connection })
    }
    pub fn list(&self) -> Result<Vec<TaskSummary>, String> {
        let mut statement = self
            .connection
            .prepare("SELECT document FROM tasks ORDER BY rowid")
            .map_err(db_error)?;
        let jsons = statement
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(db_error)?;
        let mut docs = Vec::new();
        for json in jsons {
            let document: TaskDocument = serde_json::from_str(&json.map_err(db_error)?)
                .map_err(|_| "本地任务格式损坏，未覆盖原文件".to_string())?;
            docs.push(document.summary());
        }
        Ok(docs)
    }
    pub fn load(&self, id: &str) -> Result<TaskDocument, String> {
        let json = self
            .connection
            .query_row("SELECT document FROM tasks WHERE id=?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(db_error)?
            .ok_or("任务不存在或已被删除")?;
        serde_json::from_str(&json).map_err(|_| "无法读取任务内容，原文件已保留".into())
    }
    pub fn insert(&mut self, doc: &TaskDocument) -> Result<(), String> {
        let json = serde_json::to_string(doc).map_err(|_| "无法编码任务数据")?;
        let transaction = self.connection.transaction().map_err(db_error)?;
        transaction
            .execute(
                "INSERT INTO tasks(id,name,revision,document) VALUES (?1,?2,?3,?4)",
                params![doc.id, doc.name, sql_revision(doc.revision)?, json],
            )
            .map_err(db_error)?;
        transaction.execute("INSERT INTO settings(key,value) VALUES ('activeTask',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [&doc.id]).map_err(db_error)?;
        transaction.commit().map_err(db_error)
    }
    pub fn save(&mut self, doc: &TaskDocument, expected_revision: u64) -> Result<(), String> {
        let json = serde_json::to_string(doc).map_err(|_| "无法编码任务数据")?;
        let transaction = self.connection.transaction().map_err(db_error)?;
        let changed = transaction
            .execute(
                "UPDATE tasks SET name=?1,revision=?2,document=?3 WHERE id=?4 AND revision=?5",
                params![
                    doc.name,
                    sql_revision(doc.revision)?,
                    json,
                    doc.id,
                    sql_revision(expected_revision)?
                ],
            )
            .map_err(db_error)?;
        if changed != 1 {
            return Err("任务已经变化，旧的编辑未写入，请重新加载".into());
        }
        transaction.commit().map_err(db_error)
    }
    pub fn delete(&mut self, id: &str, revision: u64) -> Result<(), String> {
        let changed = self
            .connection
            .execute(
                "DELETE FROM tasks WHERE id=?1 AND revision=?2",
                params![id, sql_revision(revision)?],
            )
            .map_err(db_error)?;
        if changed == 1 {
            Ok(())
        } else {
            Err("任务已经变化，未执行删除".into())
        }
    }
    pub fn delete_select(
        &mut self,
        id: &str,
        revision: u64,
        next_id: &str,
        fallback: Option<&TaskDocument>,
    ) -> Result<(), String> {
        let transaction = self.connection.transaction().map_err(db_error)?;
        let changed = transaction
            .execute(
                "DELETE FROM tasks WHERE id=?1 AND revision=?2",
                params![id, sql_revision(revision)?],
            )
            .map_err(db_error)?;
        if changed != 1 {
            return Err("任务已经变化，未执行删除".into());
        }
        if let Some(doc) = fallback {
            let json = serde_json::to_string(doc).map_err(|_| "无法编码新任务")?;
            transaction
                .execute(
                    "INSERT INTO tasks(id,name,revision,document) VALUES (?1,?2,?3,?4)",
                    params![doc.id, doc.name, sql_revision(doc.revision)?, json],
                )
                .map_err(db_error)?;
        }
        transaction.execute("INSERT INTO settings(key,value) VALUES ('activeTask',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [next_id]).map_err(db_error)?;
        transaction.commit().map_err(db_error)
    }
    pub fn setting(&self, key: &str) -> Result<Option<String>, String> {
        self.connection
            .query_row("SELECT value FROM settings WHERE key=?1", [key], |row| {
                row.get(0)
            })
            .optional()
            .map_err(db_error)
    }
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.connection.execute("INSERT INTO settings(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [key, value]).map_err(db_error)?;
        Ok(())
    }
    pub fn hotkeys(&self) -> Result<Hotkeys, String> {
        self.setting("hotkeys")?
            .map(|json| serde_json::from_str(&json).map_err(|_| "快捷键配置损坏".into()))
            .unwrap_or_else(|| Ok(Hotkeys::default()))
    }
}
fn db_error(error: rusqlite::Error) -> String {
    format!("本地保存失败，原任务未替换：{error}")
}

fn sql_revision(value: u64) -> Result<i64, String> {
    i64::try_from(value).map_err(|_| "任务修订号超出存储范围".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{EditRequest, RawRecording};
    use crate::event::{EventKind, RecordedEvent};
    #[test]
    fn saves_raw_current_revision_and_reopens_without_overwriting_stale_edits() {
        let dir = std::env::temp_dir().join(format!("repeat-store-{}", crate::document::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("tasks.sqlite3");
        let mut doc = TaskDocument::new("原节奏".into(), Default::default(), "test".into());
        doc.replace_recording(RawRecording {
            gestures: Default::default(),
            started_at_wall_ms: 0,
            duration_us: 3_000_000,
            geometry: Default::default(),
            layout_klid: "test".into(),
            events: vec![
                RecordedEvent {
                    seq: 1,
                    time_us: 1_000_000,
                    native_time_ms: 5,
                    kind: EventKind::KeyDown {
                        vk: 65,
                        scan: 30,
                        extended: false,
                    },
                },
                RecordedEvent {
                    seq: 2,
                    time_us: 1_100_000,
                    native_time_ms: 6,
                    kind: EventKind::KeyUp {
                        vk: 65,
                        scan: 30,
                        extended: false,
                    },
                },
            ],
        });
        let mut store = Store::open(&path).unwrap();
        store.insert(&doc).unwrap();
        let original = doc.clone();
        doc.apply(EditRequest::InsertWait {
            after_action: Some(doc.actions[0].id.clone()),
            duration_us: 1_000_000,
        })
        .unwrap();
        doc.revision = 2;
        store.save(&doc, 1).unwrap();
        assert!(store.save(&original, 1).is_err());
        drop(store);
        let mut store = Store::open(&path).unwrap();
        let loaded = store.load(&doc.id).unwrap();
        assert_eq!(loaded.duration_us, 4_000_000);
        assert_eq!(loaded.raw.unwrap().duration_us, 3_000_000);
        assert_eq!(loaded.revision, 2);
        let other = TaskDocument::new("独立任务".into(), Default::default(), "test".into());
        store.insert(&other).unwrap();
        assert_eq!(store.list().unwrap().len(), 2);
        assert_eq!(store.load(&other.id).unwrap().duration_us, 0);
        drop(store);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn failed_save_retains_committed_document() {
        let mut store = Store::open(Path::new(":memory:")).unwrap();
        let doc = TaskDocument::new("保留".into(), Default::default(), "test".into());
        store.insert(&doc).unwrap();
        store.connection.execute_batch("CREATE TRIGGER reject_write BEFORE UPDATE ON tasks BEGIN SELECT RAISE(ABORT, 'test write failure'); END;").unwrap();
        let mut changed = doc.clone();
        changed.name = "不应写入".into();
        changed.revision += 1;
        assert!(store.save(&changed, doc.revision).is_err());
        assert_eq!(store.load(&doc.id).unwrap().name, "保留");
    }
    #[test]
    fn newer_database_version_is_rejected_without_modification() {
        let path = std::env::temp_dir().join(format!(
            "repeat-future-{}.sqlite3",
            crate::document::new_id()
        ));
        {
            let connection = Connection::open(&path).unwrap();
            connection.execute_batch("PRAGMA user_version=9; CREATE TABLE keep_original(value TEXT); INSERT INTO keep_original VALUES ('unchanged');").unwrap();
        }
        assert!(Store::open(&path).is_err());
        {
            let connection = Connection::open(&path).unwrap();
            let version: u32 = connection
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(version, 9);
            let text: String = connection
                .query_row("SELECT value FROM keep_original", [], |r| r.get(0))
                .unwrap();
            assert_eq!(text, "unchanged");
        }
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn failed_delete_selection_rolls_back_the_whole_transaction() {
        let path = std::env::temp_dir().join(format!(
            "repeat-delete-{}.sqlite3",
            crate::document::new_id()
        ));
        let mut store = Store::open(&path).unwrap();
        let doc = TaskDocument::new("原任务".into(), Default::default(), "test".into());
        store.insert(&doc).unwrap();
        store.connection.execute_batch("CREATE TRIGGER fail_select BEFORE UPDATE ON settings BEGIN SELECT RAISE(FAIL,'verification'); END;").unwrap();
        let fallback = TaskDocument::new("替补".into(), Default::default(), "test".into());
        assert!(
            store
                .delete_select(&doc.id, doc.revision, &fallback.id, Some(&fallback))
                .is_err()
        );
        assert_eq!(store.list().unwrap().len(), 1);
        assert_eq!(store.setting("activeTask").unwrap().unwrap(), doc.id);
        assert!(store.load(&doc.id).is_ok());
        assert!(store.load(&fallback.id).is_err());
        drop(store);
        let _ = std::fs::remove_file(path);
    }
}
