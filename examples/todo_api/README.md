# Todo App - Roast Language Demo

An interactive command-line Todo application demonstrating Roast's features.

## Features

- ✅ **Add todos** - Create new tasks with descriptions
- ✅ **List todos** - View all tasks with completion status
- ✅ **Mark done** - Complete tasks by ID
- ✅ **Statistics** - View completion percentage
- ✅ **File persistence** - Save todos to `/tmp/todos.txt`

## How to Run

```bash
# Build the app
cd /home/swadhin/lang/roast
./target/release/roastc build examples/todo_api/main.roast -o /tmp/todo_app

# Run it
/tmp/todo_app
```

## Usage

```
==================================================
       TODO APP - Roast Language Demo
==================================================

Commands:
  1 - List all todos
  2 - Add new todo
  3 - Mark todo done
  4 - Show statistics
  5 - Save and exit

Enter command (1-5): 
```

## Language Features Demonstrated

| Feature | Usage |
|---------|-------|
| Classes | `Todo` class with methods |
| Dict storage | `todos[id] = Todo(...)` |
| File I/O | `open/write/close` |
| F-strings | `f"Added: {title}"` |
| Input | `input("")` for user interaction |
| While loops | Interactive menu loop |
| Range iteration | `for id in range(1, next_id)` |
