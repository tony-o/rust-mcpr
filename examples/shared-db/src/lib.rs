pub use rusqlite;
use rusqlite::Connection;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Book {
    pub id: i64,
    pub title: String,
    pub author: String,
    pub year: i64,
    pub summary: String,
}

pub fn db_path() -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../shared.db"))
}

pub fn open() -> Connection {
    Connection::open(db_path()).expect("failed to open shared.db")
}

fn row_to_book(row: &rusqlite::Row) -> rusqlite::Result<Book> {
    Ok(Book {
        id: row.get(0)?,
        title: row.get(1)?,
        author: row.get(2)?,
        year: row.get(3)?,
        summary: row.get(4)?,
    })
}

pub fn list_books(conn: &Connection) -> Vec<Book> {
    let mut stmt = conn
        .prepare("SELECT id, title, author, year, summary FROM books ORDER BY id")
        .expect("failed to prepare list_books");
    stmt.query_map((), row_to_book)
        .expect("failed to query list_books")
        .filter_map(Result::ok)
        .collect()
}

pub fn get_book(conn: &Connection, id: i64) -> Option<Book> {
    conn.query_row(
        "SELECT id, title, author, year, summary FROM books WHERE id = ?1",
        [id],
        row_to_book,
    )
    .ok()
}

pub fn search_books(conn: &Connection, query: &str) -> Vec<Book> {
    let pattern = format!("%{}%", query);
    let mut stmt = conn
        .prepare(
            "SELECT id, title, author, year, summary FROM books
             WHERE title LIKE ?1 OR author LIKE ?1
             ORDER BY id",
        )
        .expect("failed to prepare search_books");
    stmt.query_map([pattern], row_to_book)
        .expect("failed to query search_books")
        .filter_map(Result::ok)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queries_the_committed_database() {
        let conn = open();
        let books = list_books(&conn);
        assert_eq!(books.len(), 6);

        let hits = search_books(&conn, "Gibson");
        assert!(hits.iter().any(|b| b.title == "Neuromancer"));

        let first = get_book(&conn, books[0].id).expect("first book should be gettable");
        assert_eq!(first.title, books[0].title);
    }
}
