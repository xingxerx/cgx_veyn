1. Implement `MemoryStore::get` and `MemoryStore::delete` in `veyn-core/src/memory.rs`
   - Add SQL queries to `get_memory_by_id` and `delete_memory_by_id`
   - Update `MemoryStore` struct with `get` and `delete` methods calling the SQL queries.
2. Implement `GET /v1/memory/{id}` and `DELETE /v1/memory/{id}` endpoints in `veyn-core/src/api/routes.rs`
   - Add handler functions `memory_get` and `memory_delete` using axum routing.
   - Register the endpoints in the version 1 (`v1`) routes.
3. Complete pre commit steps
   - Complete pre commit steps to make sure proper testing, verifications, reviews and reflections are done.
4. Submit the change.
   - Once all tests pass, I will submit the change with a descriptive commit message.
