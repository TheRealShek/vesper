use rusqlite::Connection;

fn main() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE foo (id INTEGER PRIMARY KEY)").unwrap();
    
    let tx = conn.unchecked_transaction().unwrap();
    conn.execute("INSERT INTO foo (id) VALUES (1)", []).unwrap();
    tx.commit().unwrap();
}
