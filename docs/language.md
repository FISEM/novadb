# nova

Query language for novadb. Pipeline steps, Python expressions.

**Everything is a stream of records.** Each step takes the records from the
step above it and passes records down.

```
person
    where age > 30
    sort age down
    take 10
    show name, age
```

Indentation means "belongs to the line above". Anything indented fits on one
line instead — steps separated by `|`, fields by `,` inside `{ }`:

```
person | where age > 30 | show name
```

Comments start with `#`. Statements are separated by newlines or `;`.

---

## Steps

| Step | Does |
|---|---|
| `where` | keeps matching records |
| `show` | keeps, renames, or computes fields |
| `sort` | orders — `up` by default, `down` to reverse |
| `take` / `skip` | keeps / drops the first few |
| `unique` | drops repeats |
| `join … on …` | pairs with records from elsewhere |
| `group by` | splits into groups |
| `follow` / `keep following` | walks a link one step / all the way |
| `set` / `delete` | changes / removes records |

```
person | where age > 30
person | where 18 < age < 65                  # chained comparison
person | where device in ["mobile", "tablet"]
person | where nickname is None
person | where nickname                       # non-empty

person | show name, age
person | show name, years: age                # rename
person | show name, adult: age >= 18          # compute

person | sort age                             # smallest first
person | sort age down
person | sort dept, age down                  # ties broken left to right
person | sort age | skip 10 | take 10         # page two

person | show dept | unique
```

`sort` sees records before `show` drops fields, so it can sort on a field
that is not in the output.

### join

```
person
    join pet on pet.owner_id == person.id
    show person.name, pet: pet.name
```

Add `keep all` to keep records with no match, their other side reading as
`None`:

```
person | join pet on pet.owner_id == person.id keep all
```

### group by

`group by` splits the stream; the `show` after it sees each group as one
record.

```
pet
    group by species
    show species, n: count()
```

```
pet
    group by species
    show species, oldest: highest(age), usual: average(age)
    where n > 1
```

Counting words: `count()`, `total(x)`, `average(x)`, `lowest(x)`,
`highest(x)`. Filter groups with `where` after the `show`.

### follow

Links live in a collection called `edges`, holding `from_id`, `to_id` and
`label`. It is an ordinary collection: `edges | where label == "knows"`
reads it like anything else.

A link lands on a record of the collection the pipeline started from, so
`follow` walks within one collection. Reaching another one is not in v0.

```
person | where id == 1 | follow knows | show name
person | where id == 1 | follow knows | follow knows | show name
person | where id == 1 | keep following knows | show name
person | where id == 1 | follow knows backward | show name
```

`keep following` goes breadth-first, visits each record once, and stops on
loops.

---

## Writing

```
add person
    id: 1
    name: "alice"
    age: 30

add person { id: 2, name: "bob" }
```

`set` and `delete` are last steps:

```
person | where id == 1 | set age = 31
person | where id == 1 | set age = age + 1, seen = True
person | where age < 18 | delete
person | delete                               # every record
```

Drop the last step to preview what it would do.

---

## Shapes

```
define person
    id: number key
    name: string
    age: number?

define session                                # no body: any shape
```

| Type | Holds |
|---|---|
| `number` | any number |
| `string` | text |
| `boolean` | `True` or `False` |
| `any` | anything |

`?` means the field may be missing or `None`. `key` marks the identifying
field. A shape is a claim, not a constraint — nothing rejects a record that
disagrees.

```
remove person
remove person if exists
```

---

## Names

`define … as` names a piece of a pipeline.

```
define adults as
    person
        where age >= 18

adults | where city == "Paris"
```

A name starting with a step is something you pipe into:

```
define recent as
    where created > 1000
    sort created down

session | recent | take 10
```

Names are re-read every time, never stored. They share one list of names
with collections, and cannot lead back to themselves. `remove` deletes one.

There are no parameters — add a step instead: `adults | where age > 30`.

---

## Built-in collections

The database describes itself. These are read-only.

```
collections       # name, shaped, records
fields            # collection, name, type, optional, key
queries           # name, body
```

```
fields | where collection == "person" | show name, type

collections
    join fields on fields.collection == collections.name
    group by collections.name
    show name, how_many: count()
```

---

## Expressions

| | |
|---|---|
| Arithmetic | `+` `-` `*` `/` `%` |
| Comparison | `==` `!=` `<` `<=` `>` `>=`, chainable |
| Logic | `and` `or` `not` |
| Membership | `in`, `not in` |
| Nothing | `is None`, `is not None` |
| Fields | `p.name`, `p.address.city` |
| Indexing | `tags[0]`, `payload["items"]` |

```
1     1.5     "text"     'text'     True     False     None
[1, 2, 3]     { a: 1, b: 2 }
```

Both quote characters make a string.

```
len(name)     name.upper()     name.lower()
name.startswith("a")           name.endswith("z")
abs(n)        round(n)         number(x)     string(x)
```

`a or b` returns the first thing that is there, so there is no `COALESCE`.

**Truthiness** is Python's: `None`, `False`, `0`, `""`, `[]`, `{}` are
false.

**`None`** is an ordinary value. `None == None` is true, and sorting puts
`None` first. There is no three-valued logic.

---

## Errors

Errors point at the word, say what is wrong in a sentence, and name the fix.

```
person | where aeg > 30
               ^^^
person has no field 'aeg'. Did you mean 'age'?
```

```
person | show name | sort age
                          ^^^
'age' was dropped by 'show name' on the step above.
Move 'sort age' before it, or add age to the show.
```

---

## Not in v0

Parameters on a name · queries inside queries · `add` from a query ·
transactions · shape enforcement.

Why any of this is the way it is: [design-notes.md](design-notes.md).
