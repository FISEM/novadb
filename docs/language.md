# nova — the query language

> **Status: design draft.** Nothing here is implemented yet. The name `nova`
> is provisional. This document is the contract the implementation will be
> built against, and the place to argue about the language before code exists
> to defend.

nova replaces SQL in novadb. It keeps what SQL readers rely on — declarative,
text-first, no driver — and drops what SQL makes awkward: documents, graph
traversal, and a type system that predates JSON.

It reads like Python. Not superficially: the operators, the truthiness, the
built-ins and the value semantics are Python's, because a developer who knows
Python should be able to guess right.

---

## 1. The whole language in one idea

**Everything is a stream of records, and every stage takes a stream and
returns a stream.**

```
person | where age > 30 | sort age | take 10 | select name, age
```

Read it left to right: all the people, the ones over thirty, oldest last,
the first ten, their name and age. There is no clause order to memorise,
no `SELECT` that comes first but runs last. What you read is what happens.

A **record** is a document: field names to values. A collection is a source
of records. That is the entire data model — a typed table is a collection
whose records happen to agree on their shape.

### The property this buys

Delete the last stage and you have a preview of what the last stage would
have done:

```
person | where age < 18            # look at them first
person | where age < 18 | delete   # then delete exactly those
```

Destructive operations are the same query as the one that shows you what
you are about to destroy. In SQL you rewrite `SELECT` into `DELETE` and hope
the `WHERE` survived the edit.

---

## 2. Reading data

### Sources

A bare name is a collection:

```
person
```

That alone is a valid query: every record in `person`.

### `where` — keep matching records

```
person | where age > 30
person | where age > 30 and name.startswith("a")
person | where 18 < age < 65                    # chained, as in Python
person | where device in ["mobile", "tablet"]
person | where nickname is None
```

### `select` — choose and rename fields

```
person | select name
person | select name, age
person | select name, years: age                # rename
person | select name, is_adult: age >= 18       # compute
```

Without `select`, records pass through whole. `select` never has to come
first, so a query is written in the order you think of it.

### `sort`, `take`, `skip`

```
person | sort age
person | sort age desc
person | sort dept, age desc                    # ties broken left to right
person | sort age | take 10
person | sort age | skip 10 | take 10           # page two
```

`sort` sees the records as they reach it, so it can sort on a field a later
`select` drops. This is deliberate: it is the bug that SQL's clause ordering
invites, and the pipeline makes it impossible to write by accident.

### `distinct`

```
person | select dept | distinct
```

---

## 3. Combining and grouping

### `join`

```
person | join pet on pet.owner_id == person.id
       | select person.name, pet: pet.name

person | join pet on pet.owner_id == person.id keep all
       | select person.name, pet: pet.name
```

`keep all` is the left join: people with no pet still come through, with
`pet.*` reading as `None`. The default drops them.

### `group`

`group` partitions the stream. The `select` that follows sees each group as
one record, and aggregate functions fold it:

```
pet | group species | select species, n: count()
pet | group species | select species, oldest: max(age), mean: avg(age)
pet | group species | select species, n: count() | where n > 1
```

Filtering groups is just `where` after the `select` — there is no `HAVING`
to learn, because there is nothing for it to do that `where` does not.

Aggregates: `count()`, `sum(x)`, `avg(x)`, `min(x)`, `max(x)`.

---

## 4. Graph traversal

Edges live in an ordinary collection you control. `walk` follows them:

```
person | where id == 1 | walk knows | select name             # one hop
person | where id == 1 | walk knows | walk knows | select name  # two hops
person | where id == 1 | walk knows* | select name            # any depth
person | where id == 1 | walk back knows | select name        # incoming
```

`walk knows*` is breadth-first, visits each record once, and terminates on
cycles. `walk back` follows edges in reverse — spelled as a word rather than
a symbol so nothing collides with a negative number.

Traversal is a stage like any other, so it composes:

```
person | where id == 1
       | walk knows*
       | where age > 30
       | sort name
       | select name
```

---

## 5. Writing data

### `insert`

```
insert person { id: 1, name: "alice", age: 30 }
insert person { id: 2, name: "bob" }, { id: 3, name: "carol" }
```

### `set` and `delete` — terminal stages

```
person | where id == 1 | set age = 31
person | where id == 1 | set age = age + 1, seen: True
person | where age < 18 | delete
person | delete                                 # every record, said plainly
```

A `set` or `delete` with no `where` in front of it is not a mistake the
language hides: you wrote a pipeline over the whole collection, and it reads
that way.

### `define` — declaring shape

```
define person { id: int key, name: str, age: int? }
define session                                  # schemaless, any shape
```

Types: `int`, `float`, `str`, `bool`, `json`. A trailing `?` means the field
may be absent or `None`. `key` marks the primary key.

`define` with no body is a document collection: every record may have a
different shape. This is the same mechanism, not a special case — a body is
a claim about shape, and no body is no claim.

### `drop`

```
drop person
drop person if exists
```

---

## 6. Expressions

The expression language is Python's, as far as it goes.

| | |
|---|---|
| Arithmetic | `+` `-` `*` `/` `%` |
| Comparison | `==` `!=` `<` `<=` `>` `>=`, chainable: `18 < age < 65` |
| Logic | `and` `or` `not` |
| Membership | `in`, `not in` |
| Identity | `is None`, `is not None` |
| Fields | `p.name`, `p.address.city` |
| Indexing | `tags[0]`, `payload["items"]` |

### Literals

```
1        1.5        "text"        'text'        True        False        None
[1, 2, 3]                         { a: 1, b: 2 }
```

**Both quote characters make a string.** This is the one place nova breaks
with SQL on purpose. novadb's own README documents SQL's double-quote rule as
the trap newcomers fall into; a language that claims to be easy does not get
to keep a trap it has already identified.

### Built-ins

Python's names, not SQL's:

```
len(name)        name.upper()      name.lower()
name.startswith("a")               name.endswith("z")
abs(n)           round(n)          int(x)      str(x)     float(x)
```

`coalesce(a, b)` becomes `a or b`, because Python's `or` already returns the
first truthy operand.

### Truthiness

Python's, exactly: `None`, `False`, `0`, `""`, `[]` and `{}` are false;
everything else is true.

```
person | where nickname                         # has a non-empty nickname
```

This is not a new rule to teach — novadb's engine already evaluates
truthiness this way today.

### `None`

`None` is an ordinary value. `None == None` is true, and sorting puts `None`
before everything else.

nova has **no three-valued logic**. SQL's `NULL = NULL` being neither true
nor false is a documented source of confusion, and the engine already does
not implement it. A language that claims to be easy should not import SQL's
hardest rule to hold in your head.

---

## 7. Statements

A query is one statement. Several are separated by newlines or `;`, and each
returns its own result:

```
define person { id: int key, name: str }
insert person { id: 1, name: "alice" }
person | select name
```

Comments start with `#`.

---

## 8. What is deliberately not here, yet

Named so the absences are choices rather than oversights:

- **Subqueries and `let` bindings.** A pipeline can only branch by joining.
  Some queries want a named intermediate; that needs a syntax and has not
  earned one yet.
- **`insert` from a query.** `insert archive (session | where ts < 1000)`
  is obvious and useful, and is left out of v0 only to keep `insert` one
  shape.
- **Transactions.** Unchanged from today: none.
- **Schema enforcement.** `define` states shape; nothing rejects a record
  that disagrees. Same as novadb today, and the same honest caveat applies.

---

## 9. Open questions

Things this draft decides one way and could reasonably decide the other.

1. **`sort` before `select`.** The spec says `sort` sees pre-projection
   records. It reads naturally, but it means `person | select name | sort age`
   cannot work — `age` is gone by then. Is that the right error, or should
   `sort` reach back?
2. **`select` with a bare name list vs. a record literal.** `select name, age`
   is lighter; `select { name, age }` is more obviously "build this record"
   and matches `insert`. The spec picks the lighter one.
3. **`keep all` for left joins.** Reads well, but it is a two-word keyword
   in a language with none. `left join` is uglier and instantly understood.
4. **Aggregates outside `group`.** `pet | select n: count()` over the whole
   collection — allowed, or does it require an explicit empty `group`?
5. **The name.** `nova` collides with the database. A language usually wants
   its own name.
