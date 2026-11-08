//! Collections: the user's own game lists (step 7; docs/07 "Collections").
//!
//! A collection holds primary game files. It is a catalog list only: adding,
//! removing or deleting a collection never touches games or files. A collection
//! can be chosen as "what to copy" in a deployment plan.
//!
//! Kinds (ADR-073): `list` (the user's own), `favorites` (built in, id `favorites`,
//! always listed first; the star on a game) and `smart` (a saved library filter: its
//! games are whatever matches now, so it fills itself as the library grows).

use crate::catalog::{self, GameQuery};
use crate::db;
use std::collections::HashSet;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;

#[derive(Debug, Serialize, Clone, PartialEq)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub games: i64,
    pub bytes: i64,
    pub created_at: String,
    /// `list`, `favorites` or `smart`.
    pub kind: String,
    /// A smart collection's saved filter.
    pub rule: Option<GameQuery>,
}

pub const FAVORITES: &str = "favorites";
/// Smart collections match at most this many games.
const SMART_CAP: usize = 200_000;

/// The Favorites collection exists (created the first time it's needed).
fn ensure_favorites(connection: &Connection) -> Result<(), String> {
    connection
        .execute("INSERT OR IGNORE INTO collections (id, name, kind) VALUES (?1, 'Favorites', 'favorites')", [FAVORITES])
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Primary file ids in Favorites.
pub fn favorite_file_ids(connection: &Connection) -> Result<HashSet<String>, String> {
    let mut statement = connection.prepare("SELECT game_file_id FROM collection_games WHERE collection_id = ?1").map_err(|e| e.to_string())?;
    let rows = statement.query_map([FAVORITES], |r| r.get(0)).map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Star or unstar a game.
pub fn set_favorite(connection: &Connection, file_id: &str, favorite: bool) -> Result<(), String> {
    ensure_favorites(connection)?;
    let ids = [file_id.to_string()];
    if favorite { add(connection, FAVORITES, &ids) } else { remove(connection, FAVORITES, &ids) }.map(|_| ())
}

fn kind_and_rule(connection: &Connection, id: &str) -> Result<Option<(String, Option<String>)>, String> {
    connection.query_row("SELECT kind, rule_json FROM collections WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))).optional().map_err(|e| e.to_string())
}

/// A filter as saved in a smart collection: only the filters, no paging, sorting or
/// collection of its own.
fn clean_rule(query: &GameQuery) -> GameQuery {
    GameQuery { collection_id: None, offset: 0, limit: None, sort: None, left_out: false, ..query.clone() }
}

/// A library query with a smart collection replaced by its saved filter (the other
/// filters in `query` narrow it further).
pub fn resolve(connection: &Connection, query: &GameQuery) -> Result<GameQuery, String> {
    let Some(id) = query.collection_id.as_deref().filter(|c| !c.is_empty()) else { return Ok(query.clone()) };
    let Some((kind, rule)) = kind_and_rule(connection, id)? else { return Ok(query.clone()) };
    if kind != "smart" {
        return Ok(query.clone());
    }
    let rule: GameQuery = rule.and_then(|r| serde_json::from_str(&r).ok()).unwrap_or_default();
    let pick = |mine: &Option<String>, saved: &Option<String>| mine.clone().filter(|v| !v.trim().is_empty()).or_else(|| saved.clone());
    Ok(GameQuery {
        collection_id: None,
        text: pick(&query.text, &rule.text),
        platform: pick(&query.platform, &rule.platform),
        availability: pick(&query.availability, &rule.availability),
        storage_id: pick(&query.storage_id, &rule.storage_id),
        letter: pick(&query.letter, &rule.letter),
        genre: pick(&query.genre, &rule.genre),
        year_from: query.year_from.or(rule.year_from),
        year_to: query.year_to.or(rule.year_to),
        duplicates_only: query.duplicates_only || rule.duplicates_only,
        lightgun_only: query.lightgun_only || rule.lightgun_only,
        vertical_only: query.vertical_only || rule.vertical_only,
        ..query.clone()
    })
}

/// The primary file ids in a collection (a smart one: what its filter matches now).
pub fn member_file_ids(connection: &Connection, id: &str, presence: catalog::PresenceFn) -> Result<HashSet<String>, String> {
    if kind_and_rule(connection, id)?.is_some_and(|(kind, _)| kind == "smart") {
        let query = GameQuery { collection_id: Some(id.to_string()), ..GameQuery::default() };
        return Ok(catalog::matching_file_ids(connection, &query, presence, SMART_CAP)?.into_iter().collect());
    }
    let mut statement = connection.prepare("SELECT game_file_id FROM collection_games WHERE collection_id = ?1").map_err(|e| e.to_string())?;
    let rows = statement.query_map([id], |r| r.get(0)).map_err(|e| e.to_string())?;
    rows.collect::<Result<_, _>>().map_err(|e| e.to_string())
}

/// Save the current library filters as a smart collection.
pub fn create_smart(connection: &Connection, name: &str, query: &GameQuery) -> Result<String, String> {
    let rule = clean_rule(query);
    if rule == GameQuery::default() {
        return Err("Choose at least one filter (platform, genre, years, light gun, vertical or a search) first.".into());
    }
    let id = create(connection, name)?;
    let json = serde_json::to_string(&rule).map_err(|e| e.to_string())?;
    connection.execute("UPDATE collections SET kind = 'smart', rule_json = ?2 WHERE id = ?1", params![id, json]).map_err(|e| e.to_string())?;
    Ok(id)
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

/// All collections with their game counts (left-out games aren't counted):
/// Favorites first, then the rest by name. A smart collection counts what its
/// filter matches now.
pub fn list(connection: &Connection, presence: catalog::PresenceFn) -> Result<Vec<Collection>, String> {
    ensure_favorites(connection)?;
    let mut statement = connection
        .prepare(
            "SELECT c.id, c.name, COUNT(gf.id), COALESCE(SUM(gf.payload_bytes), 0), c.created_at, c.kind, c.rule_json
             FROM collections c
             LEFT JOIN collection_games cg ON cg.collection_id = c.id
             LEFT JOIN game_files gf ON gf.id = cg.game_file_id AND gf.excluded_at IS NULL
             GROUP BY c.id ORDER BY c.kind <> 'favorites', c.name COLLATE NOCASE",
        )
        .map_err(|e| e.to_string())?;
    let rows: Vec<(Collection, Option<String>)> = statement
        .query_map([], |r| {
            Ok((Collection { id: r.get(0)?, name: r.get(1)?, games: r.get(2)?, bytes: r.get(3)?, created_at: r.get(4)?, kind: r.get(5)?, rule: None }, r.get(6)?))
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<_, _>>()
        .map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for (mut c, rule) in rows {
        if c.kind == "smart" {
            c.rule = rule.and_then(|r| serde_json::from_str(&r).ok());
            let ids = member_file_ids(connection, &c.id, presence)?;
            c.games = ids.len() as i64;
            let json = serde_json::to_string(&ids.iter().collect::<Vec<_>>()).map_err(|e| e.to_string())?;
            c.bytes = connection.query_row("SELECT COALESCE(SUM(payload_bytes), 0) FROM game_files WHERE id IN (SELECT value FROM json_each(?1))", [json], |r| r.get(0)).map_err(|e| e.to_string())?;
        }
        out.push(c);
    }
    Ok(out)
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
    if id == FAVORITES {
        return Err("Favorites keeps its name.".into());
    }
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
    if id == FAVORITES {
        return Err("Favorites can't be deleted; unstar games to take them out.".into());
    }
    connection.execute("DELETE FROM collections WHERE id = ?1", [id]).map(|n| n > 0).map_err(|e| e.to_string())
}

/// Add primary game files to a collection. Returns how many were new to it.
pub fn add(connection: &Connection, id: &str, file_ids: &[String]) -> Result<i64, String> {
    match kind_and_rule(connection, id)? {
        None => return Err("Collection not found".into()),
        Some((kind, _)) if kind == "smart" => return Err("A smart collection fills itself from its filters, so games can't be added by hand.".into()),
        Some(_) => {}
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
    if kind_and_rule(connection, id)?.is_some_and(|(kind, _)| kind == "smart") {
        return Err("A smart collection fills itself from its filters; change its filters instead.".into());
    }
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
        let listed: Vec<Collection> = list(&c, &connected).unwrap().into_iter().filter(|x| x.kind != "favorites").collect();
        assert_eq!((listed[0].name.as_str(), listed[0].games), ("Couch co-op", 2));

        let page = catalog::query_games(&c, &GameQuery { collection_id: Some(couch.clone()), ..GameQuery::default() }, &connected).unwrap();
        assert_eq!(page.total, 2);
        assert_eq!(remove(&c, &couch, &nes[..1]).unwrap(), 1);
        rename(&c, &couch, "Party").unwrap();
        assert_eq!(list(&c, &connected).unwrap()[1].name, "Party", "Favorites first");
        assert!(delete(&c, &couch).unwrap());
        assert_eq!(catalog::library_stats(&c, &connected).unwrap().files, 3, "deleting a collection never removes games");
        assert!(root.join("nes/Kid.nes").exists());
    }

    #[test]
    fn favorites_are_a_built_in_collection() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("roms");
        fs::create_dir_all(root.join("nes")).unwrap();
        fs::write(root.join("nes/Kid.nes"), b"k").unwrap();
        fs::write(root.join("nes/Metroid.nes"), b"m").unwrap();
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        scanner::scan(&mut c, &root, &|_: &str| VolumeProbe::unknown()).unwrap();
        let first = list(&c, &connected).unwrap();
        assert_eq!((first[0].id.as_str(), first[0].name.as_str(), first[0].kind.as_str(), first[0].games), (FAVORITES, "Favorites", "favorites", 0));
        let page = catalog::query_games(&c, &GameQuery::default(), &connected).unwrap();
        let kid = page.rows.iter().find(|r| r.title.contains("Kid")).unwrap().file_id.clone();
        set_favorite(&c, &kid, true).unwrap();
        set_favorite(&c, &kid, true).unwrap();
        assert_eq!(list(&c, &connected).unwrap()[0].games, 1, "starring twice counts once");
        let page = catalog::query_games(&c, &GameQuery::default(), &connected).unwrap();
        assert!(page.rows.iter().all(|r| r.favorite == r.title.contains("Kid")));
        let favs = catalog::query_games(&c, &GameQuery { collection_id: Some(FAVORITES.into()), ..GameQuery::default() }, &connected).unwrap();
        assert_eq!(favs.rows.len(), 1);
        assert!(rename(&c, FAVORITES, "Mine").is_err() && delete(&c, FAVORITES).is_err());
        set_favorite(&c, &kid, false).unwrap();
        assert_eq!(list(&c, &connected).unwrap()[0].games, 0);
    }

    #[test]
    fn smart_collections_fill_themselves_from_their_filters() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("roms");
        for rel in ["nes/Kid.nes", "nes/Metroid.nes", "snes/Mario.sfc"] {
            let p = root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, rel.as_bytes()).unwrap();
        }
        let mut c = Connection::open_in_memory().unwrap();
        db::migrate(&mut c, None).unwrap();
        scanner::scan(&mut c, &root, &|_: &str| VolumeProbe::unknown()).unwrap();
        assert!(create_smart(&c, "Everything", &GameQuery::default()).is_err(), "needs at least one filter");
        let nes = GameQuery { platform: Some("NES".into()), offset: 40, sort: Some("size".into()), ..GameQuery::default() };
        let id = create_smart(&c, "All NES", &nes).unwrap();
        let listed = list(&c, &connected).unwrap();
        let smart = listed.iter().find(|x| x.id == id).unwrap();
        assert_eq!((smart.kind.as_str(), smart.games), ("smart", 2));
        assert_eq!(smart.rule.as_ref().map(|r| (r.offset, r.sort.clone())), Some((0, None)), "only the filters are saved");
        // Viewing it applies the filter; other filters narrow it further.
        let view = catalog::query_games(&c, &GameQuery { collection_id: Some(id.clone()), ..GameQuery::default() }, &connected).unwrap();
        assert_eq!(view.rows.len(), 2);
        let narrowed = catalog::query_games(&c, &GameQuery { collection_id: Some(id.clone()), text: Some("Metroid".into()), ..GameQuery::default() }, &connected).unwrap();
        assert_eq!(narrowed.rows.len(), 1);
        // It fills itself: a new NES game appears without adding it.
        fs::write(root.join("nes/Zelda.nes"), b"z").unwrap();
        scanner::scan(&mut c, &root, &|_: &str| VolumeProbe::unknown()).unwrap();
        assert_eq!(member_file_ids(&c, &id, &connected).unwrap().len(), 3);
        assert!(add(&c, &id, &["x".to_string()]).is_err() && remove(&c, &id, &["x".to_string()]).is_err(), "no hand edits");
    }
}

