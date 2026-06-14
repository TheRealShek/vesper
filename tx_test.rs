use rusqlite::Connection;

fn check(conn: &Connection) {
    let tx = conn.unchecked_transaction().unwrap();
}
