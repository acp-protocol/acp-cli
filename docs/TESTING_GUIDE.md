# Testing Guide: Tree-Sitter AST Parsing & Git2 Integration

A comprehensive guide for testing the ACP CLI's tree-sitter AST parsing and git2 integration features.

---

## Overview

This guide covers testing for:
- **AST Parsing**: Tree-sitter based symbol extraction for 6 languages
- **Git2 Integration**: Repository operations, blame tracking, file history
- **Indexer Integration**: Combined AST + git metadata in cache generation

---

## Prerequisites

```bash
# Ensure Rust is installed
rustc --version  # Should be 1.70+

# Navigate to CLI directory
cd cli

# Build the project
cargo build

# Verify dependencies
cargo tree | grep -E "tree-sitter|git2"
```

---

## 1. Running the Automated Test Suite

### Run All Tests

```bash
# From cli/ directory
cargo test

# Expected: 63 tests passing
# - 48 unit tests
# - 14 integration tests
# - 1 doc test
```

### Run Specific Test Categories

```bash
# AST parsing tests only
cargo test ast::

# Git integration tests only
cargo test git::

# Language-specific tests
cargo test typescript
cargo test rust::
cargo test python
cargo test go::
cargo test java
cargo test javascript
```

### Run with Verbose Output

```bash
cargo test -- --nocapture
```

---

## 2. Testing AST Parsing by Language

### 2.1 TypeScript

**Test file**: Create `test_sample.ts`

```typescript
// Function with generics
export function processItems<T>(items: T[]): T[] {
    return items.filter(item => item !== null);
}

// Interface
export interface User {
    id: number;
    name: string;
}

// Class with methods
export class UserService {
    private users: User[] = [];

    async getUser(id: number): Promise<User | undefined> {
        return this.users.find(u => u.id === id);
    }
}

// Arrow function
export const formatName = (first: string, last: string): string => {
    return `${first} ${last}`;
};

// Type alias
export type UserId = number;
```

**Expected extractions**:
- Function: `processItems` (generic, exported)
- Interface: `User`
- Class: `UserService`
- Method: `getUser` (async)
- Arrow function: `formatName`
- Type alias: `UserId`

### 2.2 JavaScript

**Test file**: Create `test_sample.js`

```javascript
// Regular function
export function calculateTotal(items) {
    return items.reduce((sum, item) => sum + item.price, 0);
}

// Class
export class ShoppingCart {
    constructor() {
        this.items = [];
    }

    addItem(item) {
        this.items.push(item);
    }
}

// Arrow function
export const formatPrice = (price) => `$${price.toFixed(2)}`;

// Async function
export async function fetchProducts() {
    return await fetch('/api/products');
}
```

**Expected extractions**:
- Function: `calculateTotal`
- Class: `ShoppingCart`
- Method: `addItem`
- Arrow function: `formatPrice`
- Async function: `fetchProducts`

### 2.3 Rust

**Test file**: Create `test_sample.rs`

```rust
//! Module documentation

/// A user struct
pub struct User {
    pub id: u64,
    pub name: String,
}

/// User trait
pub trait Identifiable {
    fn id(&self) -> u64;
}

impl Identifiable for User {
    fn id(&self) -> u64 {
        self.id
    }
}

/// Status enum
pub enum Status {
    Active,
    Inactive,
    Pending,
}

/// Process users
pub fn process_users(users: Vec<User>) -> Vec<u64> {
    users.iter().map(|u| u.id).collect()
}

/// Async fetch
pub async fn fetch_user(id: u64) -> Option<User> {
    None
}
```

**Expected extractions**:
- Struct: `User` (with fields)
- Trait: `Identifiable`
- Impl: `Identifiable for User`
- Enum: `Status` (with variants)
- Function: `process_users`
- Async function: `fetch_user`

### 2.4 Python

**Test file**: Create `test_sample.py`

```python
"""Module docstring"""

class User:
    """User class"""

    def __init__(self, name: str, email: str):
        self.name = name
        self.email = email

    def get_display_name(self) -> str:
        """Get display name"""
        return f"{self.name} <{self.email}>"


def process_users(users: list[User]) -> list[str]:
    """Process a list of users"""
    return [u.get_display_name() for u in users]


async def fetch_user(user_id: int) -> User | None:
    """Async fetch user"""
    return None


CONSTANT_VALUE = 42
```

**Expected extractions**:
- Class: `User`
- Methods: `__init__`, `get_display_name`
- Function: `process_users`
- Async function: `fetch_user`
- Constant: `CONSTANT_VALUE`

### 2.5 Go

**Test file**: Create `test_sample.go`

```go
package main

// User represents a user
type User struct {
    ID   int64
    Name string
}

// Identifiable interface
type Identifiable interface {
    GetID() int64
}

// GetID implements Identifiable
func (u *User) GetID() int64 {
    return u.ID
}

// ProcessUsers processes users
func ProcessUsers(users []User) []int64 {
    ids := make([]int64, len(users))
    for i, u := range users {
        ids[i] = u.ID
    }
    return ids
}

// fetchUser is private
func fetchUser(id int64) *User {
    return nil
}
```

**Expected extractions**:
- Struct: `User` (with fields)
- Interface: `Identifiable`
- Method: `GetID` (receiver: `*User`)
- Function: `ProcessUsers` (public)
- Function: `fetchUser` (private)

### 2.6 Java

**Test file**: Create `TestSample.java`

```java
package com.example;

/**
 * User class
 */
public class User {
    private Long id;
    private String name;

    public User(Long id, String name) {
        this.id = id;
        this.name = name;
    }

    public Long getId() {
        return id;
    }
}

/**
 * User service interface
 */
public interface UserService {
    User getUser(Long id);
    void saveUser(User user);
}

/**
 * Status enum
 */
public enum Status {
    ACTIVE,
    INACTIVE,
    PENDING
}
```

**Expected extractions**:
- Class: `User`
- Constructor: `User(Long, String)`
- Method: `getId`
- Interface: `UserService`
- Interface methods: `getUser`, `saveUser`
- Enum: `Status`

---

## 3. Testing Git Integration

### 3.1 Repository Operations

```bash
# Test from any git repository
cd /path/to/git/repo

# The tests automatically use the current directory
cargo test git::repository
```

**Manual verification**:
```rust
use acp::git::GitRepository;

let repo = GitRepository::open(".").unwrap();

// Get HEAD commit
let commit = repo.head_commit().unwrap();
println!("HEAD: {}", commit);  // 40-char SHA

// Get short commit
let short = repo.head_commit_short().unwrap();
println!("Short: {}", short);  // 7-char SHA

// Get current branch
let branch = repo.current_branch().unwrap();
println!("Branch: {:?}", branch);  // Some("main") or None if detached
```

### 3.2 Blame Tracking

```bash
cargo test git::blame
```

**Manual verification**:
```rust
use acp::git::{GitRepository, BlameInfo};
use std::path::Path;

let repo = GitRepository::open(".").unwrap();
let blame = BlameInfo::for_file(&repo, Path::new("Cargo.toml")).unwrap();

// Get line count
println!("Lines with blame: {}", blame.line_count());

// Get specific line
if let Some(line_blame) = blame.get_line(1) {
    println!("Line 1 author: {}", line_blame.author);
    println!("Line 1 commit: {}", line_blame.commit);
}

// Get last modified for a range
if let Some(last) = blame.last_modified(1, 10) {
    println!("Lines 1-10 last modified by: {}", last.author);
}
```

### 3.3 File History

```bash
cargo test git::history
```

**Manual verification**:
```rust
use acp::git::{GitRepository, FileHistory};
use std::path::Path;

let repo = GitRepository::open(".").unwrap();
let history = FileHistory::for_file(&repo, Path::new("Cargo.toml"), 10).unwrap();

// Get commit count
println!("Commits: {}", history.commit_count());

// Get contributors
println!("Contributors: {:?}", history.contributors());

// Get latest commit
if let Some(latest) = history.latest() {
    println!("Latest commit: {}", latest.commit_short);
    println!("Author: {}", latest.author);
    println!("Message: {}", latest.message);
}
```

---

## 4. Testing the Integrated Indexer

### 4.1 Index a Project

```bash
# Build release for better performance
cargo build --release

# Index the CLI itself
./target/release/acp index .

# Check generated cache
cat .acp.cache.json | jq '.stats'
```

**Expected output structure**:
```json
{
  "stats": {
    "files": 25,
    "symbols": 150,
    "lines": 5000
  }
}
```

### 4.2 Verify Git Metadata in Cache

```bash
# Check git commit in cache
cat .acp.cache.json | jq '.git_commit'

# Check file git info
cat .acp.cache.json | jq '.files["src/lib.rs"].git'
```

**Expected file git info**:
```json
{
  "last_commit": "abc123...",
  "last_author": "Author Name",
  "last_modified": "2025-01-15T10:30:00Z",
  "commit_count": 42,
  "contributors": ["Author1", "Author2"]
}
```

### 4.3 Verify Symbol Git Info

```bash
cat .acp.cache.json | jq '.symbols | to_entries | .[0].value.git'
```

**Expected symbol git info**:
```json
{
  "last_commit": "abc123...",
  "last_author": "Author Name",
  "code_age_days": 30
}
```

### 4.4 Verify AST-Extracted Symbols

```bash
# Count symbols by type
cat .acp.cache.json | jq '[.symbols[].type] | group_by(.) | map({type: .[0], count: length})'
```

**Expected types**:
- `function`
- `method`
- `class`
- `struct`
- `trait`
- `enum`
- `interface`

---

## 5. Performance Testing

### Benchmark Indexing

```bash
# Time the indexing
time cargo run --release -- index /path/to/large/project

# Compare with/without git metadata
# (Git operations are sequential, AST parsing is parallel)
```

### Memory Usage

```bash
# Monitor memory during indexing
/usr/bin/time -l cargo run --release -- index .
```

---

## 6. Schema Validation

### Validate Generated Cache

```bash
# Run schema validation tests
cargo test schema_validation

# Manual validation with external tool
npx ajv validate -s schemas/v1/cache.schema.json -d .acp.cache.json
```

---

## 7. Troubleshooting

### Common Issues

**Issue**: `UnsupportedLanguage` error
- **Cause**: File extension not recognized
- **Solution**: Check `AstParser::supported_extensions()`

**Issue**: Git operations fail
- **Cause**: Not in a git repository or file not tracked
- **Solution**: Ensure running from git repo root

**Issue**: Empty symbols array
- **Cause**: AST parsing failed silently
- **Solution**: Check file encoding, ensure valid syntax

### Debug Mode

```bash
# Enable debug logging
RUST_LOG=debug cargo run -- index .

# Trace AST parsing
RUST_LOG=acp::ast=trace cargo run -- index .
```

---

## 8. Test Coverage Summary

| Component | Unit Tests | Integration | Manual |
|-----------|------------|-------------|--------|
| TypeScript extractor | ✓ | - | ✓ |
| JavaScript extractor | ✓ | - | ✓ |
| Rust extractor | ✓ | - | ✓ |
| Python extractor | ✓ | - | ✓ |
| Go extractor | ✓ | - | ✓ |
| Java extractor | ✓ | - | ✓ |
| GitRepository | ✓ | - | ✓ |
| BlameInfo | ✓ | - | ✓ |
| FileHistory | ✓ | - | ✓ |
| Indexer integration | - | ✓ | ✓ |
| Schema validation | - | ✓ | ✓ |

---

## 9. Quick Verification Checklist

```bash
# 1. All tests pass
cargo test

# 2. Build succeeds
cargo build --release

# 3. Index works
./target/release/acp index .

# 4. Cache is valid JSON
cat .acp.cache.json | jq . > /dev/null && echo "Valid JSON"

# 5. Git metadata present
cat .acp.cache.json | jq '.git_commit' | grep -q null || echo "Git commit present"

# 6. Symbols extracted
cat .acp.cache.json | jq '.stats.symbols' | grep -v "^0$" && echo "Symbols found"
```

All checks passing indicates successful implementation.
