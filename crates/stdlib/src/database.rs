//! Database support.
//!
//! SQLite bindings and connection pooling.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

// =============================================================================
// Error Types
// =============================================================================

/// Database error.
#[derive(Debug, Clone)]
pub enum DbError {
    /// Connection error.
    Connection(String),
    /// Query error.
    Query(String),
    /// Type conversion error.
    Type(String),
    /// Transaction error.
    Transaction(String),
    /// Pool exhausted.
    PoolExhausted,
    /// Not found.
    NotFound,
    /// Constraint violation.
    Constraint(String),
    /// IO error.
    Io(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::Connection(msg) => write!(f, "Connection error: {}", msg),
            DbError::Query(msg) => write!(f, "Query error: {}", msg),
            DbError::Type(msg) => write!(f, "Type error: {}", msg),
            DbError::Transaction(msg) => write!(f, "Transaction error: {}", msg),
            DbError::PoolExhausted => write!(f, "Connection pool exhausted"),
            DbError::NotFound => write!(f, "Not found"),
            DbError::Constraint(msg) => write!(f, "Constraint violation: {}", msg),
            DbError::Io(msg) => write!(f, "IO error: {}", msg),
        }
    }
}

impl std::error::Error for DbError {}

pub type DbResult<T> = Result<T, DbError>;

// =============================================================================
// Value Types
// =============================================================================

/// Database value.
#[derive(Debug, Clone)]
pub enum Value {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl Value {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Integer(i) => Some(*i),
            _ => None,
        }
    }
    
    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Real(f) => Some(*f),
            Value::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }
    
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Value::Text(s) => Some(s),
            _ => None,
        }
    }
    
    pub fn as_blob(&self) -> Option<&[u8]> {
        match self {
            Value::Blob(b) => Some(b),
            _ => None,
        }
    }
    
    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }
}

impl From<i64> for Value {
    fn from(i: i64) -> Self {
        Value::Integer(i)
    }
}

impl From<f64> for Value {
    fn from(f: f64) -> Self {
        Value::Real(f)
    }
}

impl From<String> for Value {
    fn from(s: String) -> Self {
        Value::Text(s)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Self {
        Value::Text(s.to_string())
    }
}

impl From<Vec<u8>> for Value {
    fn from(b: Vec<u8>) -> Self {
        Value::Blob(b)
    }
}

impl<T: Into<Value>> From<Option<T>> for Value {
    fn from(opt: Option<T>) -> Self {
        match opt {
            Some(v) => v.into(),
            None => Value::Null,
        }
    }
}

// =============================================================================
// Row
// =============================================================================

/// A database row.
#[derive(Debug, Clone)]
pub struct Row {
    columns: Vec<String>,
    values: Vec<Value>,
}

impl Row {
    pub fn new(columns: Vec<String>, values: Vec<Value>) -> Self {
        Self { columns, values }
    }
    
    /// Get value by column name.
    pub fn get(&self, column: &str) -> Option<&Value> {
        self.columns.iter()
            .position(|c| c == column)
            .map(|i| &self.values[i])
    }
    
    /// Get value by index.
    pub fn get_idx(&self, idx: usize) -> Option<&Value> {
        self.values.get(idx)
    }
    
    /// Get column names.
    pub fn columns(&self) -> &[String] {
        &self.columns
    }
    
    /// Get all values.
    pub fn values(&self) -> &[Value] {
        &self.values
    }
    
    /// Get integer by column.
    pub fn get_int(&self, column: &str) -> Option<i64> {
        self.get(column).and_then(|v| v.as_int())
    }
    
    /// Get float by column.
    pub fn get_float(&self, column: &str) -> Option<f64> {
        self.get(column).and_then(|v| v.as_float())
    }
    
    /// Get text by column.
    pub fn get_text(&self, column: &str) -> Option<&str> {
        self.get(column).and_then(|v| v.as_text())
    }
    
    /// Get blob by column.
    pub fn get_blob(&self, column: &str) -> Option<&[u8]> {
        self.get(column).and_then(|v| v.as_blob())
    }
    
    /// Convert to hashmap.
    pub fn to_map(&self) -> HashMap<String, Value> {
        self.columns.iter()
            .cloned()
            .zip(self.values.iter().cloned())
            .collect()
    }
}

// =============================================================================
// Query Builder
// =============================================================================

/// SQL query builder.
#[derive(Debug, Clone)]
pub struct QueryBuilder {
    query: String,
    params: Vec<Value>,
}

impl QueryBuilder {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            params: Vec::new(),
        }
    }
    
    /// SELECT query.
    pub fn select(columns: &[&str]) -> Self {
        Self {
            query: format!("SELECT {}", columns.join(", ")),
            params: Vec::new(),
        }
    }
    
    /// FROM clause.
    pub fn from(mut self, table: &str) -> Self {
        self.query.push_str(&format!(" FROM {}", table));
        self
    }
    
    /// WHERE clause.
    pub fn where_eq(mut self, column: &str, value: impl Into<Value>) -> Self {
        self.query.push_str(&format!(" WHERE {} = ?", column));
        self.params.push(value.into());
        self
    }
    
    /// AND condition.
    pub fn and_eq(mut self, column: &str, value: impl Into<Value>) -> Self {
        self.query.push_str(&format!(" AND {} = ?", column));
        self.params.push(value.into());
        self
    }
    
    /// OR condition.
    pub fn or_eq(mut self, column: &str, value: impl Into<Value>) -> Self {
        self.query.push_str(&format!(" OR {} = ?", column));
        self.params.push(value.into());
        self
    }
    
    /// ORDER BY clause.
    pub fn order_by(mut self, column: &str, asc: bool) -> Self {
        let dir = if asc { "ASC" } else { "DESC" };
        self.query.push_str(&format!(" ORDER BY {} {}", column, dir));
        self
    }
    
    /// LIMIT clause.
    pub fn limit(mut self, n: i64) -> Self {
        self.query.push_str(&format!(" LIMIT {}", n));
        self
    }
    
    /// OFFSET clause.
    pub fn offset(mut self, n: i64) -> Self {
        self.query.push_str(&format!(" OFFSET {}", n));
        self
    }
    
    /// INSERT query.
    pub fn insert(table: &str) -> Self {
        Self {
            query: format!("INSERT INTO {}", table),
            params: Vec::new(),
        }
    }
    
    /// VALUES clause.
    pub fn values(mut self, columns: &[&str], values: Vec<Value>) -> Self {
        let placeholders: Vec<_> = (0..values.len()).map(|_| "?").collect();
        self.query.push_str(&format!(
            " ({}) VALUES ({})",
            columns.join(", "),
            placeholders.join(", ")
        ));
        self.params = values;
        self
    }
    
    /// UPDATE query.
    pub fn update(table: &str) -> Self {
        Self {
            query: format!("UPDATE {}", table),
            params: Vec::new(),
        }
    }
    
    /// SET clause.
    pub fn set(mut self, column: &str, value: impl Into<Value>) -> Self {
        if self.query.contains(" SET ") {
            self.query.push_str(&format!(", {} = ?", column));
        } else {
            self.query.push_str(&format!(" SET {} = ?", column));
        }
        self.params.push(value.into());
        self
    }
    
    /// DELETE query.
    pub fn delete(table: &str) -> Self {
        Self {
            query: format!("DELETE FROM {}", table),
            params: Vec::new(),
        }
    }
    
    /// Raw SQL.
    pub fn raw(sql: &str) -> Self {
        Self {
            query: sql.to_string(),
            params: Vec::new(),
        }
    }
    
    /// Add a parameter.
    pub fn param(mut self, value: impl Into<Value>) -> Self {
        self.params.push(value.into());
        self
    }
    
    /// Build the query.
    pub fn build(self) -> (String, Vec<Value>) {
        (self.query, self.params)
    }
    
    /// Get the SQL string.
    pub fn sql(&self) -> &str {
        &self.query
    }
    
    /// Get parameters.
    pub fn params(&self) -> &[Value] {
        &self.params
    }
}

impl Default for QueryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Connection (Interface)
// =============================================================================

/// Database connection interface.
pub trait Connection: Send + Sync {
    /// Execute a query.
    fn execute(&self, sql: &str, params: &[Value]) -> DbResult<u64>;
    
    /// Query rows.
    fn query(&self, sql: &str, params: &[Value]) -> DbResult<Vec<Row>>;
    
    /// Query single row.
    fn query_one(&self, sql: &str, params: &[Value]) -> DbResult<Option<Row>> {
        let rows = self.query(sql, params)?;
        Ok(rows.into_iter().next())
    }
    
    /// Begin transaction.
    fn begin(&self) -> DbResult<()>;
    
    /// Commit transaction.
    fn commit(&self) -> DbResult<()>;
    
    /// Rollback transaction.
    fn rollback(&self) -> DbResult<()>;
    
    /// Check if in transaction.
    fn in_transaction(&self) -> bool;
}

// =============================================================================
// Mock Connection (for testing)
// =============================================================================

/// Mock connection for testing.
pub struct MockConnection {
    in_tx: Mutex<bool>,
    data: Mutex<Vec<Row>>,
}

impl MockConnection {
    pub fn new() -> Self {
        Self {
            in_tx: Mutex::new(false),
            data: Mutex::new(Vec::new()),
        }
    }
    
    pub fn with_rows(rows: Vec<Row>) -> Self {
        Self {
            in_tx: Mutex::new(false),
            data: Mutex::new(rows),
        }
    }
}

impl Connection for MockConnection {
    fn execute(&self, _sql: &str, _params: &[Value]) -> DbResult<u64> {
        Ok(1)
    }
    
    fn query(&self, _sql: &str, _params: &[Value]) -> DbResult<Vec<Row>> {
        Ok(self.data.lock().unwrap().clone())
    }
    
    fn begin(&self) -> DbResult<()> {
        *self.in_tx.lock().unwrap() = true;
        Ok(())
    }
    
    fn commit(&self) -> DbResult<()> {
        *self.in_tx.lock().unwrap() = false;
        Ok(())
    }
    
    fn rollback(&self) -> DbResult<()> {
        *self.in_tx.lock().unwrap() = false;
        Ok(())
    }
    
    fn in_transaction(&self) -> bool {
        *self.in_tx.lock().unwrap()
    }
}

impl Default for MockConnection {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Connection Pool
// =============================================================================

/// Connection pool.
pub struct Pool<C: Connection + 'static> {
    connections: Mutex<Vec<Arc<C>>>,
    max_size: usize,
    factory: Box<dyn Fn() -> DbResult<C> + Send + Sync>,
}

impl<C: Connection + 'static> Pool<C> {
    /// Create a new pool.
    pub fn new<F>(max_size: usize, factory: F) -> Self
    where
        F: Fn() -> DbResult<C> + Send + Sync + 'static,
    {
        Self {
            connections: Mutex::new(Vec::new()),
            max_size,
            factory: Box::new(factory),
        }
    }
    
    /// Get a connection from the pool.
    pub fn get(&self) -> DbResult<PooledConnection<C>> {
        let mut conns = self.connections.lock().unwrap();
        
        // Try to get an existing connection
        if let Some(conn) = conns.pop() {
            return Ok(PooledConnection {
                conn: Some(conn),
                pool: self as *const Pool<C>,
            });
        }
        
        // Create a new connection if under limit
        if conns.len() < self.max_size {
            let conn = (self.factory)()?;
            return Ok(PooledConnection {
                conn: Some(Arc::new(conn)),
                pool: self as *const Pool<C>,
            });
        }
        
        Err(DbError::PoolExhausted)
    }
    
    /// Return a connection to the pool.
    fn return_connection(&self, conn: Arc<C>) {
        let mut conns = self.connections.lock().unwrap();
        if conns.len() < self.max_size {
            conns.push(conn);
        }
    }
    
    /// Get pool size.
    pub fn size(&self) -> usize {
        self.connections.lock().unwrap().len()
    }
}

/// A pooled connection that returns to pool when dropped.
pub struct PooledConnection<C: Connection + 'static> {
    conn: Option<Arc<C>>,
    pool: *const Pool<C>,
}

impl<C: Connection> std::ops::Deref for PooledConnection<C> {
    type Target = C;
    
    fn deref(&self) -> &Self::Target {
        self.conn.as_ref().unwrap()
    }
}

impl<C: Connection> Drop for PooledConnection<C> {
    fn drop(&mut self) {
        if let Some(conn) = self.conn.take() {
            // Safety: pool pointer is valid for the lifetime of the pool
            let pool = unsafe { &*self.pool };
            pool.return_connection(conn);
        }
    }
}

// =============================================================================
// Transaction Helper
// =============================================================================

/// Execute a function in a transaction.
pub fn transaction<C, F, R>(conn: &C, f: F) -> DbResult<R>
where
    C: Connection,
    F: FnOnce(&C) -> DbResult<R>,
{
    conn.begin()?;
    
    match f(conn) {
        Ok(result) => {
            conn.commit()?;
            Ok(result)
        }
        Err(e) => {
            let _ = conn.rollback();
            Err(e)
        }
    }
}

// =============================================================================
// Schema Types
// =============================================================================

/// Column type.
#[derive(Debug, Clone, Copy)]
pub enum ColumnType {
    Integer,
    Real,
    Text,
    Blob,
}

impl ColumnType {
    pub fn sql(&self) -> &str {
        match self {
            ColumnType::Integer => "INTEGER",
            ColumnType::Real => "REAL",
            ColumnType::Text => "TEXT",
            ColumnType::Blob => "BLOB",
        }
    }
}

/// Column definition.
#[derive(Debug, Clone)]
pub struct ColumnDef {
    pub name: String,
    pub column_type: ColumnType,
    pub nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub default: Option<Value>,
}

impl ColumnDef {
    pub fn new(name: &str, column_type: ColumnType) -> Self {
        Self {
            name: name.to_string(),
            column_type,
            nullable: true,
            primary_key: false,
            unique: false,
            default: None,
        }
    }
    
    pub fn not_null(mut self) -> Self {
        self.nullable = false;
        self
    }
    
    pub fn primary_key(mut self) -> Self {
        self.primary_key = true;
        self
    }
    
    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }
    
    pub fn default(mut self, value: impl Into<Value>) -> Self {
        self.default = Some(value.into());
        self
    }
    
    pub fn sql(&self) -> String {
        let mut s = format!("{} {}", self.name, self.column_type.sql());
        
        if self.primary_key {
            s.push_str(" PRIMARY KEY");
        }
        if !self.nullable {
            s.push_str(" NOT NULL");
        }
        if self.unique {
            s.push_str(" UNIQUE");
        }
        if let Some(ref default) = self.default {
            s.push_str(&format!(" DEFAULT {}", value_to_sql(default)));
        }
        
        s
    }
}

fn value_to_sql(v: &Value) -> String {
    match v {
        Value::Null => "NULL".to_string(),
        Value::Integer(i) => i.to_string(),
        Value::Real(f) => f.to_string(),
        Value::Text(s) => format!("'{}'", s.replace('\'', "''")),
        Value::Blob(_) => "X''".to_string(),
    }
}

/// Table definition.
#[derive(Debug, Clone)]
pub struct TableDef {
    pub name: String,
    pub columns: Vec<ColumnDef>,
}

impl TableDef {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            columns: Vec::new(),
        }
    }
    
    pub fn column(mut self, col: ColumnDef) -> Self {
        self.columns.push(col);
        self
    }
    
    pub fn create_sql(&self, if_not_exists: bool) -> String {
        let cols: Vec<_> = self.columns.iter().map(|c| c.sql()).collect();
        let exists = if if_not_exists { " IF NOT EXISTS" } else { "" };
        format!("CREATE TABLE{} {} ({})", exists, self.name, cols.join(", "))
    }
    
    pub fn drop_sql(&self, if_exists: bool) -> String {
        let exists = if if_exists { " IF EXISTS" } else { "" };
        format!("DROP TABLE{} {}", exists, self.name)
    }
}

// =============================================================================
// In-Memory Database
// =============================================================================

/// In-memory database for testing and lightweight use.
pub struct MemoryDb {
    tables: Mutex<HashMap<String, MemoryTable>>,
    in_tx: Mutex<bool>,
    tx_snapshot: Mutex<Option<HashMap<String, MemoryTable>>>,
}

#[derive(Clone)]
struct MemoryTable {
    columns: Vec<String>,
    column_types: Vec<ColumnType>,
    rows: Vec<Vec<Value>>,
    auto_id: i64,
}

impl MemoryDb {
    pub fn new() -> Self {
        Self {
            tables: Mutex::new(HashMap::new()),
            in_tx: Mutex::new(false),
            tx_snapshot: Mutex::new(None),
        }
    }
    
    /// Create a table.
    pub fn create_table(&self, def: &TableDef) -> DbResult<()> {
        let mut tables = self.tables.lock().unwrap();
        
        if tables.contains_key(&def.name) {
            return Err(DbError::Query(format!("Table {} already exists", def.name)));
        }
        
        tables.insert(def.name.clone(), MemoryTable {
            columns: def.columns.iter().map(|c| c.name.clone()).collect(),
            column_types: def.columns.iter().map(|c| c.column_type).collect(),
            rows: Vec::new(),
            auto_id: 1,
        });
        
        Ok(())
    }
    
    /// Drop a table.
    pub fn drop_table(&self, name: &str) -> DbResult<()> {
        let mut tables = self.tables.lock().unwrap();
        tables.remove(name);
        Ok(())
    }
    
    /// Insert a row.
    pub fn insert(&self, table: &str, columns: &[&str], values: Vec<Value>) -> DbResult<i64> {
        let mut tables = self.tables.lock().unwrap();
        
        let t = tables.get_mut(table)
            .ok_or_else(|| DbError::Query(format!("Table {} not found", table)))?;
        
        if columns.len() != values.len() {
            return Err(DbError::Query("Column count mismatch".to_string()));
        }
        
        // Build full row with nulls for missing columns
        let mut row = vec![Value::Null; t.columns.len()];
        
        for (col, val) in columns.iter().zip(values) {
            if let Some(idx) = t.columns.iter().position(|c| c == *col) {
                row[idx] = val;
            }
        }
        
        // Auto-increment ID if first column is integer and null
        if let Some(Value::Null) = row.first() {
            row[0] = Value::Integer(t.auto_id);
            t.auto_id += 1;
        }
        
        let id = if let Some(Value::Integer(id)) = row.first() {
            *id
        } else {
            0
        };
        
        t.rows.push(row);
        Ok(id)
    }
    
    /// Select rows.
    pub fn select(
        &self,
        table: &str,
        columns: &[&str],
        predicate: Option<Box<dyn Fn(&Row) -> bool>>,
    ) -> DbResult<Vec<Row>> {
        let tables = self.tables.lock().unwrap();
        
        let t = tables.get(table)
            .ok_or_else(|| DbError::Query(format!("Table {} not found", table)))?;
        
        let col_indices: Vec<_> = if columns.is_empty() || (columns.len() == 1 && columns[0] == "*") {
            (0..t.columns.len()).collect()
        } else {
            columns.iter()
                .filter_map(|c| t.columns.iter().position(|tc| tc == *c))
                .collect()
        };
        
        let mut results = Vec::new();
        
        for row_values in &t.rows {
            let selected_cols: Vec<_> = col_indices.iter()
                .map(|&i| t.columns[i].clone())
                .collect();
            let selected_vals: Vec<_> = col_indices.iter()
                .map(|&i| row_values[i].clone())
                .collect();
            
            let row = Row::new(selected_cols, selected_vals);
            
            if let Some(ref pred) = predicate {
                if pred(&row) {
                    results.push(row);
                }
            } else {
                results.push(row);
            }
        }
        
        Ok(results)
    }
    
    /// Update rows.
    pub fn update(
        &self,
        table: &str,
        updates: &[(&str, Value)],
        predicate: Box<dyn Fn(&Row) -> bool>,
    ) -> DbResult<u64> {
        let mut tables = self.tables.lock().unwrap();
        
        let t = tables.get_mut(table)
            .ok_or_else(|| DbError::Query(format!("Table {} not found", table)))?;
        
        let mut count = 0;
        
        for row_values in &mut t.rows {
            let row = Row::new(t.columns.clone(), row_values.clone());
            
            if predicate(&row) {
                for (col, val) in updates {
                    if let Some(idx) = t.columns.iter().position(|c| c == *col) {
                        row_values[idx] = val.clone();
                    }
                }
                count += 1;
            }
        }
        
        Ok(count)
    }
    
    /// Delete rows.
    pub fn delete(
        &self,
        table: &str,
        predicate: Box<dyn Fn(&Row) -> bool>,
    ) -> DbResult<u64> {
        let mut tables = self.tables.lock().unwrap();
        
        let t = tables.get_mut(table)
            .ok_or_else(|| DbError::Query(format!("Table {} not found", table)))?;
        
        let initial_len = t.rows.len();
        
        t.rows.retain(|row_values| {
            let row = Row::new(t.columns.clone(), row_values.clone());
            !predicate(&row)
        });
        
        Ok((initial_len - t.rows.len()) as u64)
    }
    
    /// Get table info.
    pub fn table_info(&self, table: &str) -> DbResult<Vec<(String, ColumnType)>> {
        let tables = self.tables.lock().unwrap();
        
        let t = tables.get(table)
            .ok_or_else(|| DbError::Query(format!("Table {} not found", table)))?;
        
        Ok(t.columns.iter().cloned()
            .zip(t.column_types.iter().copied())
            .collect())
    }
    
    /// List all tables.
    pub fn tables(&self) -> Vec<String> {
        self.tables.lock().unwrap().keys().cloned().collect()
    }
}

impl Default for MemoryDb {
    fn default() -> Self {
        Self::new()
    }
}

impl Connection for MemoryDb {
    fn execute(&self, _sql: &str, _params: &[Value]) -> DbResult<u64> {
        // Simple implementation - would need SQL parser
        Ok(1)
    }
    
    fn query(&self, _sql: &str, _params: &[Value]) -> DbResult<Vec<Row>> {
        // Simple implementation - would need SQL parser
        Ok(Vec::new())
    }
    
    fn begin(&self) -> DbResult<()> {
        let mut in_tx = self.in_tx.lock().unwrap();
        if *in_tx {
            return Err(DbError::Transaction("Already in transaction".to_string()));
        }
        
        // Snapshot current state
        let tables = self.tables.lock().unwrap().clone();
        *self.tx_snapshot.lock().unwrap() = Some(tables);
        *in_tx = true;
        
        Ok(())
    }
    
    fn commit(&self) -> DbResult<()> {
        let mut in_tx = self.in_tx.lock().unwrap();
        if !*in_tx {
            return Err(DbError::Transaction("Not in transaction".to_string()));
        }
        
        *self.tx_snapshot.lock().unwrap() = None;
        *in_tx = false;
        
        Ok(())
    }
    
    fn rollback(&self) -> DbResult<()> {
        let mut in_tx = self.in_tx.lock().unwrap();
        if !*in_tx {
            return Err(DbError::Transaction("Not in transaction".to_string()));
        }
        
        // Restore snapshot
        if let Some(snapshot) = self.tx_snapshot.lock().unwrap().take() {
            *self.tables.lock().unwrap() = snapshot;
        }
        *in_tx = false;
        
        Ok(())
    }
    
    fn in_transaction(&self) -> bool {
        *self.in_tx.lock().unwrap()
    }
}

// =============================================================================
// Migration System
// =============================================================================

/// A database migration.
pub struct Migration {
    pub version: i64,
    pub name: String,
    pub up: String,
    pub down: String,
}

impl Migration {
    pub fn new(version: i64, name: &str, up: &str, down: &str) -> Self {
        Self {
            version,
            name: name.to_string(),
            up: up.to_string(),
            down: down.to_string(),
        }
    }
}

/// Migration runner.
pub struct Migrator<C: Connection> {
    conn: Arc<C>,
    migrations: Vec<Migration>,
}

impl<C: Connection> Migrator<C> {
    pub fn new(conn: Arc<C>) -> Self {
        Self {
            conn,
            migrations: Vec::new(),
        }
    }
    
    /// Add a migration.
    pub fn add(mut self, migration: Migration) -> Self {
        self.migrations.push(migration);
        self.migrations.sort_by_key(|m| m.version);
        self
    }
    
    /// Initialize migrations table.
    pub fn init(&self) -> DbResult<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS _migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at TEXT NOT NULL
            )",
            &[],
        )?;
        Ok(())
    }
    
    /// Get current version.
    pub fn current_version(&self) -> DbResult<i64> {
        let rows = self.conn.query(
            "SELECT MAX(version) as version FROM _migrations",
            &[],
        )?;
        
        Ok(rows.first()
            .and_then(|r| r.get_int("version"))
            .unwrap_or(0))
    }
    
    /// Get pending migrations.
    pub fn pending(&self) -> DbResult<Vec<&Migration>> {
        let current = self.current_version()?;
        Ok(self.migrations.iter()
            .filter(|m| m.version > current)
            .collect())
    }
    
    /// Run all pending migrations.
    pub fn migrate(&self) -> DbResult<u64> {
        let pending = self.pending()?;
        let mut count = 0;
        
        for migration in pending {
            self.conn.begin()?;
            
            // Run up migration
            self.conn.execute(&migration.up, &[])?;
            
            // Record migration
            self.conn.execute(
                "INSERT INTO _migrations (version, name, applied_at) VALUES (?, ?, datetime('now'))",
                &[
                    Value::Integer(migration.version),
                    Value::Text(migration.name.clone()),
                ],
            )?;
            
            self.conn.commit()?;
            count += 1;
        }
        
        Ok(count)
    }
    
    /// Rollback the last migration.
    pub fn rollback(&self) -> DbResult<bool> {
        let current = self.current_version()?;
        
        if current == 0 {
            return Ok(false);
        }
        
        let migration = self.migrations.iter()
            .find(|m| m.version == current)
            .ok_or(DbError::NotFound)?;
        
        self.conn.begin()?;
        
        // Run down migration
        self.conn.execute(&migration.down, &[])?;
        
        // Remove migration record
        self.conn.execute(
            "DELETE FROM _migrations WHERE version = ?",
            &[Value::Integer(migration.version)],
        )?;
        
        self.conn.commit()?;
        
        Ok(true)
    }
    
    /// Rollback all migrations.
    pub fn rollback_all(&self) -> DbResult<u64> {
        let mut count = 0;
        while self.rollback()? {
            count += 1;
        }
        Ok(count)
    }
}

// =============================================================================
// Prepared Statement
// =============================================================================

/// Prepared statement.
pub struct PreparedStatement<C: Connection> {
    conn: Arc<C>,
    sql: String,
}

impl<C: Connection> PreparedStatement<C> {
    pub fn new(conn: Arc<C>, sql: &str) -> Self {
        Self {
            conn,
            sql: sql.to_string(),
        }
    }
    
    /// Execute with parameters.
    pub fn execute(&self, params: &[Value]) -> DbResult<u64> {
        self.conn.execute(&self.sql, params)
    }
    
    /// Query with parameters.
    pub fn query(&self, params: &[Value]) -> DbResult<Vec<Row>> {
        self.conn.query(&self.sql, params)
    }
    
    /// Query single row.
    pub fn query_one(&self, params: &[Value]) -> DbResult<Option<Row>> {
        self.conn.query_one(&self.sql, params)
    }
}

// =============================================================================
// ORM-like Features
// =============================================================================

/// Trait for models that can be loaded from database.
pub trait FromRow: Sized {
    fn from_row(row: &Row) -> DbResult<Self>;
}

/// Trait for models that can be saved to database.
pub trait ToRow {
    fn table_name() -> &'static str;
    fn columns() -> &'static [&'static str];
    fn values(&self) -> Vec<Value>;
}

/// Simple repository pattern.
pub struct Repository<C: Connection, T: FromRow + ToRow> {
    conn: Arc<C>,
    _marker: std::marker::PhantomData<T>,
}

impl<C: Connection, T: FromRow + ToRow> Repository<C, T> {
    pub fn new(conn: Arc<C>) -> Self {
        Self {
            conn,
            _marker: std::marker::PhantomData,
        }
    }
    
    /// Find all.
    pub fn all(&self) -> DbResult<Vec<T>> {
        let (sql, _) = QueryBuilder::select(&["*"])
            .from(T::table_name())
            .build();
        
        let rows = self.conn.query(&sql, &[])?;
        rows.iter().map(T::from_row).collect()
    }
    
    /// Find by ID.
    pub fn find(&self, id: i64) -> DbResult<Option<T>> {
        let (sql, params) = QueryBuilder::select(&["*"])
            .from(T::table_name())
            .where_eq("id", id)
            .build();
        
        let row = self.conn.query_one(&sql, &params)?;
        row.map(|r| T::from_row(&r)).transpose()
    }
    
    /// Insert.
    pub fn insert(&self, entity: &T) -> DbResult<u64> {
        let cols: Vec<_> = T::columns().iter().map(|&s| s).collect();
        let (sql, params) = QueryBuilder::insert(T::table_name())
            .values(&cols, entity.values())
            .build();
        
        self.conn.execute(&sql, &params)
    }
    
    /// Delete by ID.
    pub fn delete(&self, id: i64) -> DbResult<u64> {
        let (sql, params) = QueryBuilder::delete(T::table_name())
            .where_eq("id", id)
            .build();
        
        self.conn.execute(&sql, &params)
    }
    
    /// Count all.
    pub fn count(&self) -> DbResult<i64> {
        let sql = format!("SELECT COUNT(*) as count FROM {}", T::table_name());
        let row = self.conn.query_one(&sql, &[])?;
        Ok(row.and_then(|r| r.get_int("count")).unwrap_or(0))
    }
}

// =============================================================================
// Batch Operations
// =============================================================================

/// Batch inserter for efficient bulk inserts.
pub struct BatchInserter<C: Connection> {
    conn: Arc<C>,
    table: String,
    columns: Vec<String>,
    batch: Vec<Vec<Value>>,
    batch_size: usize,
}

impl<C: Connection> BatchInserter<C> {
    pub fn new(conn: Arc<C>, table: &str, columns: &[&str], batch_size: usize) -> Self {
        Self {
            conn,
            table: table.to_string(),
            columns: columns.iter().map(|&s| s.to_string()).collect(),
            batch: Vec::new(),
            batch_size,
        }
    }
    
    /// Add a row to the batch.
    pub fn add(&mut self, values: Vec<Value>) -> DbResult<()> {
        self.batch.push(values);
        
        if self.batch.len() >= self.batch_size {
            self.flush()?;
        }
        
        Ok(())
    }
    
    /// Flush the batch.
    pub fn flush(&mut self) -> DbResult<u64> {
        if self.batch.is_empty() {
            return Ok(0);
        }
        
        let placeholders: Vec<_> = (0..self.columns.len())
            .map(|_| "?")
            .collect();
        let placeholder_row = format!("({})", placeholders.join(", "));
        let all_placeholders: Vec<_> = (0..self.batch.len())
            .map(|_| placeholder_row.as_str())
            .collect();
        
        let sql = format!(
            "INSERT INTO {} ({}) VALUES {}",
            self.table,
            self.columns.join(", "),
            all_placeholders.join(", ")
        );
        
        let params: Vec<_> = self.batch.drain(..)
            .flat_map(|row| row)
            .collect();
        
        self.conn.execute(&sql, &params)
    }
    
    /// Finish and flush any remaining rows.
    pub fn finish(mut self) -> DbResult<u64> {
        self.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_query_builder_select() {
        let (sql, params) = QueryBuilder::select(&["id", "name"])
            .from("users")
            .where_eq("id", 1i64)
            .order_by("name", true)
            .limit(10)
            .build();
        
        assert_eq!(sql, "SELECT id, name FROM users WHERE id = ? ORDER BY name ASC LIMIT 10");
        assert_eq!(params.len(), 1);
    }
    
    #[test]
    fn test_query_builder_insert() {
        let (sql, params) = QueryBuilder::insert("users")
            .values(&["name", "email"], vec!["Alice".into(), "alice@example.com".into()])
            .build();
        
        assert_eq!(sql, "INSERT INTO users (name, email) VALUES (?, ?)");
        assert_eq!(params.len(), 2);
    }
    
    #[test]
    fn test_row() {
        let row = Row::new(
            vec!["id".to_string(), "name".to_string()],
            vec![Value::Integer(1), Value::Text("Alice".to_string())],
        );
        
        assert_eq!(row.get_int("id"), Some(1));
        assert_eq!(row.get_text("name"), Some("Alice"));
        assert!(row.get("missing").is_none());
    }
    
    #[test]
    fn test_mock_connection() {
        let conn = MockConnection::new();
        
        assert!(!conn.in_transaction());
        conn.begin().unwrap();
        assert!(conn.in_transaction());
        conn.commit().unwrap();
        assert!(!conn.in_transaction());
    }
    
    #[test]
    fn test_table_def() {
        let table = TableDef::new("users")
            .column(ColumnDef::new("id", ColumnType::Integer).primary_key())
            .column(ColumnDef::new("name", ColumnType::Text).not_null())
            .column(ColumnDef::new("email", ColumnType::Text).unique());
        
        let sql = table.create_sql(true);
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS users"));
        assert!(sql.contains("id INTEGER PRIMARY KEY"));
        assert!(sql.contains("name TEXT NOT NULL"));
        assert!(sql.contains("email TEXT UNIQUE"));
    }
    
    #[test]
    fn test_memory_db() {
        let db = MemoryDb::new();
        
        let table = TableDef::new("users")
            .column(ColumnDef::new("id", ColumnType::Integer).primary_key())
            .column(ColumnDef::new("name", ColumnType::Text));
        
        db.create_table(&table).unwrap();
        
        // Insert
        let id = db.insert("users", &["name"], vec!["Alice".into()]).unwrap();
        assert_eq!(id, 1);
        
        let id2 = db.insert("users", &["name"], vec!["Bob".into()]).unwrap();
        assert_eq!(id2, 2);
        
        // Select
        let rows = db.select("users", &["*"], None).unwrap();
        assert_eq!(rows.len(), 2);
        
        // Select with filter (get all columns so we can filter by id)
        let rows = db.select(
            "users",
            &["*"],
            Some(Box::new(|r| r.get_int("id") == Some(1))),
        ).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get_text("name"), Some("Alice"));
        
        // Delete
        let deleted = db.delete(
            "users",
            Box::new(|r| r.get_text("name") == Some("Bob")),
        ).unwrap();
        assert_eq!(deleted, 1);
        
        let rows = db.select("users", &["*"], None).unwrap();
        assert_eq!(rows.len(), 1);
    }
    
    #[test]
    fn test_memory_db_transaction() {
        let db = MemoryDb::new();
        
        let table = TableDef::new("items")
            .column(ColumnDef::new("id", ColumnType::Integer).primary_key())
            .column(ColumnDef::new("value", ColumnType::Integer));
        
        db.create_table(&table).unwrap();
        db.insert("items", &["value"], vec![Value::Integer(100)]).unwrap();
        
        // Start transaction
        db.begin().unwrap();
        db.insert("items", &["value"], vec![Value::Integer(200)]).unwrap();
        
        // Rollback
        db.rollback().unwrap();
        
        let rows = db.select("items", &["*"], None).unwrap();
        assert_eq!(rows.len(), 1);  // Only the first insert survives
    }
}

