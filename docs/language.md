# nova — the query language

> **Status: design draft.** Nothing here is implemented. The name `nova` is
> provisional. This document is the contract the implementation will be built
> against, and the place to argue about the language while arguing is still
> cheap.

nova replaces SQL in novadb.

It has one design goal, and every decision below is answerable to it:

> **A sixteen-year-old who has seen a little Python should be able to read a
> nova query and say what it does, without being taught the language first.**

That goal rules things out. Where a shorter, cleverer form would need
explaining, nova takes the longer form that does not.

---

## 1. One idea

**Everything is a stream of records. Every step takes the records from the
step above it and passes records to the step below.**

```
person
    where age > 30
    sort age
    take 10
    select name, age
```

All the people · the ones over thirty · oldest last · the first ten · their
name and age.

Nobody has to explain that. There is no clause order to memorise, nothing
that is written first but runs last. You read it top to bottom and that is
the order it happens.

A **record** is a document: field names to values. A collection is a source
of records. That is the whole data model — a typed table is just a
collection whose records agree on their shape.

### What this buys, beyond readability

Delete the last step and you get a preview of what the last step would have
done:

```
person
    where age < 18        # look at them

person
    where age < 18
    delete                # now delete exactly those
```

The destructive query is the safe query plus one line. In SQL you rewrite
`SELECT` into `DELETE` and hope the `WHERE` survived the edit.

---

## 2. Indentation

**Indentation means "this belongs to the line above."**

That is the only structural rule in the language, and it is the same rule
everywhere: pipeline steps, record bodies, schema bodies.

```
person                     define person              insert person
    where age > 30             id: int key                id: 1
    select name                name: str                  name: "alice"
```

**Anything indented can be written on one line instead.** Steps separate
with `|`, fields separate with `,` inside `{ }`:

```
person | where age > 30 | select name
define person { id: int key, name: str }
insert person { id: 1, name: "alice" }
```

Same grammar, not a second language. The one-line form exists because a
query has to survive being typed into a REPL, pasted into a `curl` body, or
embedded in a string in Python or JavaScript — places where a leading
indent is either impossible or already spoken for by the host language.

Indent with spaces or a tab, consistently within a query. A pipeline is flat
by construction, so in practice there is exactly one level of indentation
and none of Python's deep-nesting pain applies.

---

## 3. Reading data

A bare collection name is already a valid query: every record in it.

```
person
```

### `where` — keep matching records

```
person
    where age > 30
    where name.startswith("a")      # several where steps just stack
```

```
person | where 18 < age < 65                  # chained, as in Python
person | where device in ["mobile", "tablet"]
person | where nickname is None
```

### `select` — choose and rename fields

```
person | select name
person | select name, age
person | select name, years: age                # rename
person | select name, adult: age >= 18          # compute
```

Without `select`, whole records pass through. `select` never has to come
first, so a query is written in the order you think of it.

### `sort`, `take`, `skip`

```
person | sort age
person | sort age desc
person | sort dept, age desc                    # ties broken left to right
person | sort age | take 10
person | sort age | skip 10 | take 10           # page two
```

`sort` sees records as they reach it, so it can sort on a field a later
`select` drops. This is deliberate. It is exactly the bug that SQL's clause
ordering invites — and that novadb's own engine shipped with until it was
found and fixed.

### `distinct`

```
person | select dept | distinct
```

---

## 4. Combining and grouping

### `join`

```
person
    join pet on pet.owner_id == person.id
    select person.name, pet: pet.name
```

```
person
    join pet on pet.owner_id == person.id keep all
    select person.name, pet: pet.name
```

`keep all` is the left join: people with no pet still come through, with
`pet.*` reading as `None`. Without it they are dropped.

### `group by`

`group by` splits the stream into groups. The `select` after it sees each
group as one record, and the counting functions fold it:

```
pet
    group by species
    select species, n: count()
```

```
pet
    group by species
    select species, oldest: max(age), average: avg(age)
    where n > 1
```

Filtering groups is just `where` after the `select`. There is no `HAVING`
to learn, because there is nothing left for it to do.

Counting functions: `count()`, `sum(x)`, `avg(x)`, `min(x)`, `max(x)`.

**Nothing is inferred.** Cypher decides your grouping key by looking at
which terms in the projection are not aggregates, so adding a field
silently changes what the query means. In nova the grouping key is the
thing written after `group by`, and nowhere else.

---

## 5. Following links

Edges live in an ordinary collection you control. `follow` walks them:

```
person
    where id == 1
    follow knows
    select name
```

```
person | where id == 1 | follow knows | follow knows | select name   # two hops
person | where id == 1 | follow knows* | select name                 # any depth
person | where id == 1 | follow back knows | select name             # incoming
```

`follow knows*` goes breadth-first, visits each record once, and stops on
cycles. `follow back` goes against the arrow — spelled as a word so nothing
collides with a negative number.

`follow` is a step like any other, so it composes with everything else:

```
person
    where id == 1
    follow knows*
    where age > 30
    sort name
    select name
```

---

## 6. Writing data

### `insert`

```
insert person
    id: 1
    name: "alice"
    age: 30
```

```
insert person { id: 2, name: "bob" }
```

### `set` and `delete` — last steps

```
person | where id == 1 | set age = 31
person | where id == 1 | set age = age + 1, seen: True
person | where age < 18 | delete
person | delete                                  # every record, said plainly
```

A `set` or `delete` with no `where` above it is not a mistake the language
hides. You wrote a pipeline over the whole collection, and it reads that
way.

### `define` — declaring shape

```
define person
    id: int key
    name: str
    age: int?
```

```
define session          # no body: any shape, every record different
```

Types: `int`, `float`, `str`, `bool`, `json`. A trailing `?` means the field
may be missing or `None`. `key` marks the primary key.

A body is a claim about shape; no body is no claim. Document collections are
not a special case, they are the absence of one.

### `drop`

```
drop person
drop person if exists
```

---

## 7. Expressions

Python's, as far as they go.

| | |
|---|---|
| Arithmetic | `+` `-` `*` `/` `%` |
| Comparison | `==` `!=` `<` `<=` `>` `>=`, chainable: `18 < age < 65` |
| Logic | `and` `or` `not` |
| Membership | `in`, `not in` |
| Nothing | `is None`, `is not None` |
| Fields | `p.name`, `p.address.city` |
| Indexing | `tags[0]`, `payload["items"]` |

### Literals

```
1     1.5     "text"     'text'     True     False     None
[1, 2, 3]                { a: 1, b: 2 }
```

**Both quote characters make a string.** This is the one place nova breaks
with SQL on purpose. novadb's own README documents SQL's double-quote rule
as the trap newcomers fall into; a language claiming to be easy does not
keep a trap it has already named.

### Built-ins

Python's names, not SQL's:

```
len(name)        name.upper()        name.lower()
name.startswith("a")                 name.endswith("z")
abs(n)           round(n)            int(x)     str(x)     float(x)
```

There is no `COALESCE`, because `a or b` already returns the first thing
that is there.

### Truthiness

Python's, exactly: `None`, `False`, `0`, `""`, `[]` and `{}` are false,
everything else is true.

```
person | where nickname        # the ones with a non-empty nickname
```

This is not a new rule to teach. novadb's engine already evaluates
truthiness this way today.

### `None`

`None` is an ordinary value. `None == None` is true. Sorting puts `None`
first.

nova has **no three-valued logic**. In SQL, `NULL = NULL` is neither true
nor false, and `x NOT IN (…)` silently returns nothing when the list holds
one `NULL`. That rule is the single hardest thing about SQL to hold in your
head, it produces wrong answers rather than errors, and novadb's engine
already declines to implement it.

---

## 8. Errors are part of the design

For the sixteen-year-old, error messages matter more than syntax. A language
with an ordinary grammar and excellent errors is easier than the reverse.
These are a contract, not a nice-to-have:

```
person | where aeg > 30
               ^^^
person has no field 'aeg'. Did you mean 'age'?
```

```
person | select name | sort age
                            ^^^
'age' was dropped by 'select name' on the step above.
Move 'sort age' before it, or add age to the select.
```

```
person
    where age > 30
   select name
   ^
This line is indented less than the one above it, but more than 'person'.
Every step in a pipeline lines up at the same depth.
```

Three rules: point at the exact token, say what is wrong in a full sentence,
and name the fix.

---

## 9. The sixteen-year-old test

The bar this document is held to. If a query needs a paragraph of
explanation, the design is wrong, not the reader.

```
# Which of my friends' friends are over 30, oldest first?

person
    where name == "alice"
    follow knows*
    where age > 30
    sort age desc
    select name, age
```

Six lines, no punctuation to decode, no keyword that is not an ordinary
English word. Read it aloud and it is a sentence.

---

## 10. Deliberately not here, yet

Named so the absences are choices, not oversights.

- **Composition.** No functions, no named pipelines, no reusable fragments.
  This is the largest open question in the design: it is SQL's worst
  structural failure, PRQL's best idea, and nova currently has no answer.
- **Subqueries and `let` bindings.** A pipeline branches only by joining.
- **`insert` from a query.** `insert archive (session | where ts < 1000)`
  is obviously useful, and is left out of v0 to keep `insert` one shape.
- **Transactions.** Unchanged from today: none.
- **Schema enforcement.** `define` states a shape; nothing rejects a record
  that disagrees. Same as novadb today, same honest caveat.

---

## 11. Open questions

Decided one way here, and reasonably decidable the other.

1. **`select` as a word.** Clear to anyone who has seen SQL, less obvious to
   someone who has not. `show name, age` and `pick name, age` both read
   better cold. Changing it costs nothing today and everything later.
2. **`keep all` for left joins.** Reads well, but it is a two-word keyword
   in a language that otherwise has none. `left join` is uglier and instantly
   understood by anyone who has met SQL.
3. **`follow back knows`.** Plain, but the word order is awkward.
   Alternatives: `follow knows backwards`, or dropping incoming traversal
   from v0 entirely.
4. **Aggregates outside `group by`.** Is `pet | select n: count()` over the
   whole collection allowed, or does counting always require a `group by`?
5. **`sort` before `select`.** The spec says `sort` sees records before the
   projection drops fields. The alternative is to reject it and make the
   error teach the order. Section 8 shows the error either way.
6. **The name.** `nova` collides with the database it queries.
