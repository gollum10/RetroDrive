//! Collections: the user's own game lists (step 7; docs/07 "Collections").
//!
//! A collection holds primary game files. It is a catalog list only: adding,
//! removing or deleting a collection never touches games or files. A collection
//! can be chosen as "what to copy" in a deployment plan.

use crate::db;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub games: i64,
    pub bytes: i64,
    pub created_at: String,
}

fn clean_name(name: &str) -> Result<String, String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err("Give the collection a name.".into());
    }
    if name.chars().count() > 80 {
        return Err("Collection names can be up to 80 characters.".into());
    }
    Ok(name)
}

/// All collections with their game counts (left-out games aren't counted).
pub fn list(connection: &Connection) -> Result<Vec<Collection>, String> {
    let mut statement = connection
        .prepare(
            "SELECT c.id, c.name, COUNT(gf.id), COALESCE(SUM(gf.payload_bytes), 0), c.created_at
             FROM collections c
             LEFT JOIN collection_games cg ON cg.collection_id = c.id
             LEFT JOIN game_files gf ON gf.id = cg.game_file_id AND gf.excluded_at IS NULL
             GROUP BY c.id ORDER BY c.name COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |r| Ok(Collection { id: r.get(0)?, name: r.get(1)?, games: r.get(2)?, bytes: r.get(3)?, created_at: r.get(4)? }))
        .map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

pub fn create(connection: &Connection, name: &str) -> Result<String, String> {
    let name = clean_name(name)?;
    let taken: bool = connection
        .query_row("SELECT EXISTS (SELECT 1 FROM collections WHERE name = ?1 COLLATE NOCASE)", [&name], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if taken {
        return Err(format!("You already have a collection called \"{name}\"."));
    }
    let id = db::new_id();
    connection.execute("INSERT INTO collections (id, name) VALUES (?1, ?2)", params![id, name]).map_err(|e| e.to_string())?;
    Ok(id)
}

pub fn rename(connection: &Connection, id: &str, name: &str) -> Result<(), String> {
    let name = clean_name(name)?;
    let taken: bool = connection
        .query_row("SELECT EXISTS (SELECT 1 FROM collections WHERE name = ?1 COLLATE NOCASE AND id <> ?2)", params![name, id], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    if taken {
        return Err(format!("You already have a collection called \"{name}\"."));
    }
    match connection.execute("UPDATE collections SET name = ?2 WHERE id = ?1", params![id, name]).map_err(|e| e.to_string())? {
        0 => Err("Collection not found".into()),
        _ => Ok(()),
    }
}

/// Delete the list. Games and files are untouched.
pub fn delete(connection: &Connection, id: &str) -> Result<bool, String> {
    connection.execute("DELETE FROM collections WHERE id = ?1", [id]).map(|n| n > 0).map_err(|e| e.to_string())
}

/// Add primary game files to a collection. Returns how many were new to it.
pub fn add(connection: &Connection, id: &str, file_ids: &[String]) -> Result<i64, String> {
    let exists: Option<String> = connection.query_row("SELECT id FROM collections WHERE id = ?1", [id], |r| r.get(0)).optional().map_err(|e| e.to_string())?;
    if exists.is_none() {
        return Err("Collection not found".into());
    }
    let tx = connection.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut added = 0i64;
    {
        let mut insert = tx
            .prepare(
                "INSERT OR IGNORE INTO collection_games (collection_id, game_file_id)
                 SELECT ?1, id FROM game_files WHERE id = ?2 AND file_role = 'primary'",
            )
            .map_err(|e| e.to_string())?;
        for file in file_ids {
            added += insert.execute(params![id, file]).map_err(|e| e.to_string())? as i64;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(added)
}

/// Remove games from a collection (the games stay in the library).
pub fn remove(connection: &Connection, id: &str, file_ids: &[String]) -> Result<i64, String> {
    let tx = connection.unchecked_transaction().map_err(|e| e.to_string())?;
    let mut removed = 0i64;
    {
        let mut delete = tx.prepare("DELETE FROM collection_games WHERE collection_id = ?1 AND game_file_id = ?2").map_err(|e| e.to_string())?;
        for file in file_ids {
            removed += delete.execute(params![id, file]).map_err(|e| e.to_string())? as i64;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::{self, GameQuery};
    use crate::volumes::{Presence, VolumeProbe};
    use crate::scanner;
    use std::fs;

    fn connected(_: &str, _: Option<&str>) -> Presence {
        Presence::Connected
    }

    #[test]
    fn collections_hold_games_without_touching_them() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("roms");
        for (rel, body) in [("nes/Kid.nes", "k"), ("nes/Metroid.nes", "m"), ("snes/Mario.sfc", "s")] {
            let p = root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, body).unwrap();
        }
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        scanner::scan(&mut c, &root, &|_: &str| VolumeProbe::unknown()).unwrap();

        let couch = create(&c, "  Couch   co-op ").unwrap();
        assert!(create(&c, "couch co-op").unwrap_err().contains("already"));
        assert!(create(&c, "   ").is_err());

        let nes = catalog::matching_file_ids(&c, &GameQuery { platform: Some("NES".into()), ..GameQuery::default() }, &connected, 10_000).unwrap();
        assert_eq!(nes.len(), 2);
        assert_eq!(add(&c, &couch, &nes).unwrap(), 2);
        assert_eq!(add(&c, &couch, &nes).unwrap(), 0, "adding twice keeps one entry");
        let listed = list(&c).unwrap();
        assert_eq!((listed[0].name.as_str(), listed[0].games), ("Couch co-op", 2));

        let page = catalog::query_games(&c, &GameQuery { collection_id: Some(couch.clone()), ..GameQuery::default() }, &connected).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(remove(&c, &couch, &nes[..1]).unwrap(), 1);
        rename(&c, &couch, "Party").unwrap();
        assert_eq!(list(&c).unwrap()[0].name, "Party");
        assert!(delete(&c, &couch).unwrap());
        assert_eq!(catalog::library_stats(&c, &connected).unwrap().files, 3, "deleting a collection never removes games");
        assert!(root.join("nes/Kid.nes").exists());
    }
}
