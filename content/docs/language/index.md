---
title: Language Reference
template: docs
---

# Language Reference

> The type system, protocols, macros, and async/concurrency features are work in progress.
> This reference covers the stable core of the language.

---

## Comments

```fink
# end-of-line comment

---
block comment
---
```

---

## Literals

### Booleans

```fink
true
false
```

### Numbers

Integer size is inferred from the literal value and sign:

```fink
1_234_567               # u32
+1                      # i8
-1                      # i8
0xFF                    # u8
+0xFF                   # i8
0xFfFf                  # u16
0xFFFF_FFFF             # u32
0xFFFF_FFFF_FFFF_FFFF   # u64
0o_1234_5670            # octal
0b_0101_1111            # binary
```

Floats and decimals:

```fink
1.0             # f32
1.0e100_000     # f64

1.0d            # decimal — cannot mix with floats
1.0d-100        # decimal with negative exponent
```

### Strings

Single-quoted, with interpolation and multiline support:

```fink
'hello world'

'hello ${1 + 2}'

'
  multiline
  string
'
```

Escape sequences:

```fink
'\n'        # newline
'\t'        # tab
'\\'        # backslash
'\''        # single quote
'\x0f'      # hex code point
'\u{10_ff_ff}'  # Unicode code point
'\${' # literal ${
```

String blocks — no need to escape single quotes, indentation stripped:

```fink
":
  supports templating ${bar}
  no need to escape 'spam'
```

Tagged template strings pass raw parts and values to a function:

```fink
fmt'hello ${name}'
sql'SELECT * FROM users WHERE name = ${name}'
rx'(?<group>[a-z]+)'
```

Raw strings — escape sequences are not processed:

```fink
raw'foo \n \t bar'
raw":
  foo \n
  bar
```

### Tagged literals

Postfix function application for readable units:

```fink
10sec       # == sec 10
10.5min     # == min 10.5
(foo)min    # == min foo
```

### Collections

```fink
# sequences (tuple-optimized when used as such)
[]
[1, 2, 3]
seq 1, 2, 3

# records (compile-time field names)
{}
{foo: 1, bar: 2}
{foo: 1, 'ni na': 2, (key): 3}  # computed keys

# dictionaries (runtime keys)
dict {foo: 1, 'bar': 2, (key): 3}

# sets
set 1, 2, 3
ordered_set 3, 2, 1
```

---

## Operators

### Arithmetic

```fink
-a          # unary minus
a + b
a - b
a * b
a / b
a // b      # integer divide
a ** b      # power
a % b       # remainder (sign follows dividend)
a %% b      # true modulus (sign follows divisor)
a /% b      # divmod — returns [quotient, remainder]
```

### Logical

Operands must be bools, returns bool:

```fink
not a
a and b
a or b
a xor b
```

### Bitwise

```fink
~a          # not
a & b       # and
a ^ b       # xor
a >> b      # shift right
a << b      # shift left
a >>> b     # rotate right
a <<< b     # rotate left
```

### Comparison

Chainable, always returns bool:

```fink
a == b
a != b
a > b
a >= b
a < b
a <= b
a > b > c       # chained
a in b
a not in b
a >< b          # disjoint
```

### Ranges

```fink
0..10           # exclusive end
0...10          # inclusive end
'a'...'z'       # char range
start..end
(1 + 2)..(3 + 4)
```

### Spread

```fink
[head, ..tail]
[..seq1, ..seq2]    # concat

{foo: bar, ..rest}
{..rec1, ..rec2}    # merge
```

---

## Bindings and Pattern Matching

### Left-hand binding

```fink
foo = 1

[a, b] = [1, 2]
{x, y} = point
{x, y: z} = point   # bind x, rename y to z
```

Patterns can include guards:

```fink
[x, y >= 2] = [1, 2]
[is_odd head, ..tail] = [3, 4, 5]
```

Rest patterns:

```fink
[head, ..tail] = [1, 2, 3, 4]
[head, ..middle, end] = [1, 2, 3, 4]
```

String patterns:

```fink
'start ${middle} end' = 'start foo end'
# middle == ' foo '
```

Record patterns match partially; sequence patterns match exactly:

```fink
{a} = {a: 1, b: 2}      # ok — records are partial
[a, ..] = [1, 2]         # ok — explicit rest discard
```

### Right-hand binding

Capture the result of a multiline expression:

```fink
foo
  arg1
  arg2
|= result
```

### `match`

```fink
match foo:
  1: 'one'
  2: 'two'
  _: 'other'
```

Match on structure:

```fink
match foo:
  [head, ..tail]: head
  []: 'empty'
```

Match with guards:

```fink
match foo:
  n > 0 and n < 10: 'small positive ${n}'
  n > 0: 'large positive ${n}'
  even n: 'even number ${n}'
  _: 'other'
```

Match on types:

```fink
match foo:
  str s: 'its a string ${s}'
  u8 n: 'its a u8 ${n}'
```

Match on sequence and record structure:

```fink
match items:
  []: 'empty'
  [x]: 'one element'
  [x, y]: 'two elements'
  [x, ..rest]: 'head and rest'

match foobar:
  {}: 'empty'
  {foo: 1}: 'has foo = 1'
  {foo: 1, ..rest}: 'has foo = 1 and more'
```

---

## Functions

```fink
add = fn a, b:
  result = a + b
  result

# no args
greet = fn: 'hello'

# default args
greet = fn name='world': 'hello ${name}'

# pattern matching in args
foo = fn {x, y}: x + y
bar = fn [head, ..tail]: head
baz = fn arg, ..rest: arg
```

### `fn match` sugar

```fink
classify = fn match n:
  n > 0: 'positive'
  n < 0: 'negative'
  _: 'zero'
```

### Mutual recursion

Forward references at module level allow mutual recursion without special syntax:

```fink
is_even = fn n:
  match n:
    0: true
    _: is_odd n - 1

is_odd = fn n:
  match n:
    0: false
    _: is_even n - 1
```

---

## Application

Prefix application, right-to-left nesting:

```fink
log 'hello'
add 1, 2

# nested — right to left
foo bar spam ham
# == foo (bar (spam ham))

# multiline — indented args
add
  mul 2, 3
  mul 3, 4
```

Use `;` as a strong inline separator (stronger than `,`):

```fink
add mul 2, 3; mul 3, 4
# == add (mul 2, 3), (mul 3, 4)
```

### Partial application with `?`

`?` creates an anonymous function scoped to the current expression or pipe segment:

```fink
add5 = add 5, ?
add5 = ? + 5

filter is_divisible ?, 2   # == filter fn $: is_divisible $, 2
map ? * 2                  # == map fn $: $ * 2
```

`?` is transparent through sequences, records, and operators — all `?` in the same scope share one parameter:

```fink
[?, ?]              # == fn $: [$, $]
{foo: ?, bar: ?}    # == fn $: {foo: $, bar: $}
```

`(...)` is an explicit scope boundary:

```fink
foo (bar ?)         # == foo (fn $: bar $)
```

---

## Pipes

Left-to-right application:

```fink
'hello'
| capitalize
| log
# == log (capitalize 'hello')
```

Each pipe segment is its own `?` scope:

```fink
1..10
| filter ? % 2 == 0
| map ? * 2
| [..?]
|= even_nums
```

Pass result as spread arguments:

```fink
[1, 2] | add ..?
# == fn [a, b]: add a, b
```

---

## Error Handling

`try` unwraps `Ok` or propagates `Err` up the call stack:

```fink
fn foo:
  a = try bar a
  b = try baz a
  Ok a + b
```

`match` handles errors explicitly:

```fink
fn foo:
  match bar _:
    Ok x: x + 1
    Err e: log 'error: ${e}'
```

Error chaining:

```fink
fn foo:
  match bar _:
    Ok x: Ok x
    Err e: Err e, 'foo failed'
```

---

## Modules

```fink
{foo, bar} = import './foobar.fnk'
```

---

## Types *(work in progress)*

```fink
# product types
Point = type: u8, u8
Circle = type: {x: u8, y: u8, r: u8}

# sum types / variants
Result = variant T, E:
  Ok T
  Err E

Shape = variant:
  Circle {x: u8, y: u8, r: u8}
  Rect {x: u8, y: u8, w: u8, h: u8}
  Nil ()

# opaque types
UserId = type: ..u64

# generic types
Option = variant T:
  Some T
  None
```

Construction and matching:

```fink
circle = Circle {x: 1, y: 2, r: 5}
some = Some 42

match opt:
  Some x: x
  None: 0
```
