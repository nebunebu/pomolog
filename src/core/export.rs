//! Whole-database export and import as JSON. Tables are dumped column by
//! column, so the format follows the schema without a second set of types.

use super::db::{Db, Result};
use rusqlite::types::{Value as Sql, ValueRef};
use serde_json::{Map, Value, json};

/// In dependency order, so rows can be inserted with foreign keys on.
const TABLES: &[&str] = &[
    "boards",
    "lists",
    "cards",
    "sessions",
    "interruptions",
    "items",
    "habits",
    "habit_entries",
];

pub fn export(db: &Db) -> Result<Value> {
    let mut tables = Map::new();
    for t in TABLES {
        let mut st = db.conn.prepare(&format!("SELECT * FROM {t}"))?;
        let cols: Vec<String> = st.column_names().map(|c| c.to_string()).collect();
        let rows = st
            .query_map([], |r| {
                let mut m = Map::new();
                for (i, c) in cols.iter().enumerate() {
                    let v = match r.get_ref(i)? {
                        ValueRef::Null => Value::Null,
                        ValueRef::Integer(n) => json!(n),
                        ValueRef::Real(f) => json!(f),
                        ValueRef::Text(t) => json!(String::from_utf8_lossy(t)),
                        ValueRef::Blob(_) => Value::Null,
                    };
                    m.insert(c.clone(), v);
                }
                Ok(Value::Object(m))
            })?
            .collect::<Result<Vec<_>>>()?;
        tables.insert(t.to_string(), Value::Array(rows));
    }
    Ok(json!({ "app": "pomolog", "schema": db.schema_version()?, "tables": tables }))
}

#[derive(Debug)]
pub enum ImportError {
    NotAnExport,
    NewerSchema(u64),
    Db(rusqlite::Error),
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ImportError::NotAnExport => write!(f, "this file is not a Pomolog export"),
            ImportError::NewerSchema(v) => {
                write!(f, "this export was made by a newer version (schema {v})")
            }
            ImportError::Db(e) => write!(f, "database error: {e}"),
        }
    }
}

impl From<rusqlite::Error> for ImportError {
    fn from(e: rusqlite::Error) -> Self {
        ImportError::Db(e)
    }
}

/// Replaces everything in the database with the contents of an export.
/// Nothing is changed if the import fails part-way.
pub fn import(db: &mut Db, data: &Value) -> std::result::Result<usize, ImportError> {
    if data["app"] != "pomolog" || !data["tables"].is_object() {
        return Err(ImportError::NotAnExport);
    }
    let schema = data["schema"].as_u64().unwrap_or(0);
    if schema > db.schema_version()? as u64 {
        return Err(ImportError::NewerSchema(schema));
    }
    let tx = db.conn.transaction()?;
    for t in TABLES.iter().rev() {
        tx.execute(&format!("DELETE FROM {t}"), [])?;
    }
    let mut count = 0;
    for t in TABLES {
        let known: Vec<String> = {
            let st = tx.prepare(&format!("SELECT * FROM {t}"))?;
            st.column_names().map(|c| c.to_string()).collect()
        };
        for row in data["tables"][t].as_array().into_iter().flatten() {
            let Some(obj) = row.as_object() else { continue };
            // Unknown columns are ignored so old exports keep working.
            let (cols, vals): (Vec<&str>, Vec<Sql>) = obj
                .iter()
                .filter(|(k, _)| known.contains(k))
                .map(|(k, v)| {
                    let v = match v {
                        Value::Null => Sql::Null,
                        Value::Bool(b) => Sql::Integer(*b as i64),
                        Value::Number(n) => match n.as_i64() {
                            Some(i) => Sql::Integer(i),
                            None => Sql::Real(n.as_f64().unwrap_or(0.0)),
                        },
                        Value::String(s) => Sql::Text(s.clone()),
                        other => Sql::Text(other.to_string()),
                    };
                    (k.as_str(), v)
                })
                .unzip();
            if cols.is_empty() {
                continue;
            }
            let marks = vec!["?"; cols.len()].join(",");
            tx.execute(
                &format!("INSERT INTO {t}({}) VALUES({marks})", cols.join(",")),
                rusqlite::params_from_iter(vals),
            )?;
            count += 1;
        }
    }
    tx.commit()?;
    Ok(count)
}

pub fn wipe(db: &mut Db) -> Result<()> {
    let tx = db.conn.transaction()?;
    for t in TABLES.iter().rev() {
        tx.execute(&format!("DELETE FROM {t}"), [])?;
    }
    tx.commit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::*;
    use chrono::NaiveDate;

    fn populated() -> Db {
        let db = Db::open_in_memory().unwrap();
        let now = NaiveDate::from_ymd_opt(2026, 11, 16)
            .unwrap()
            .and_hms_opt(9, 0, 0)
            .unwrap();
        let b = db.add_board("Work").unwrap();
        let list = db.lists(b).unwrap()[1].id;
        let c = db.add_card(list, "Report", now).unwrap();
        db.insert_session(&SessionRecord {
            kind: Phase::Focus,
            card_id: Some(c),
            started_at: now,
            ended_at: now,
            duration_s: 1500,
            planned_s: 1500,
            completed: true,
            interruptions: vec![Interruption {
                at: now,
                kind: "pause".into(),
                reason: "x".into(),
            }],
        })
        .unwrap();
        db.save_item(&Item::new(ItemKind::Holiday, "Day off", now.date()))
            .unwrap();
        let h = db.save_habit(&Habit::new("Read", now.date())).unwrap();
        db.set_habit_entry(
            h,
            now.date(),
            HabitEntry {
                value: 1.0,
                skipped: false,
            },
        )
        .unwrap();
        db
    }

    #[test]
    fn round_trip() {
        let a = populated();
        let dump = export(&a).unwrap();
        let mut b = Db::open_in_memory().unwrap();
        b.add_board("Will be replaced").unwrap();
        let n = import(&mut b, &dump).unwrap();
        assert_eq!(n, 1 + 3 + 1 + 1 + 1 + 1 + 1 + 1);
        assert_eq!(export(&b).unwrap(), dump);
        let board = b.boards().unwrap()[0].id;
        assert_eq!(b.cards(board).unwrap()[0].spent_s, 1500);
    }

    #[test]
    fn bad_input_changes_nothing() {
        let mut db = populated();
        let before = export(&db).unwrap();
        assert!(matches!(
            import(&mut db, &json!({"hello": 1})),
            Err(ImportError::NotAnExport)
        ));
        let mut newer = before.clone();
        newer["schema"] = json!(999);
        assert!(matches!(
            import(&mut db, &newer),
            Err(ImportError::NewerSchema(999))
        ));
        // A row pointing at a board that does not exist fails and rolls back.
        let mut broken = before.clone();
        broken["tables"]["lists"][0]["board_id"] = json!(12345);
        assert!(matches!(import(&mut db, &broken), Err(ImportError::Db(_))));
        assert_eq!(export(&db).unwrap(), before);
    }

    #[test]
    fn wipe_empties_everything() {
        let mut db = populated();
        wipe(&mut db).unwrap();
        assert!(db.boards().unwrap().is_empty());
        assert!(db.habits().unwrap().is_empty());
        assert!(db.items().unwrap().is_empty());
    }
}
