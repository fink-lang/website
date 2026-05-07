---
template: home
---

```fink
{stdout, write} = import 'std/io.fnk'

main = fn foo:
  msg = match foo:
    'ƒink': 'little bird'
    _:      '${foo}'

  write stdout, 'Hello ${msg}!'
```

```fink
# Pattern matching with destructuring
classify = fn match n:
  n > 0: 'positive'
  n < 0: 'negative'
  _:  'zero'

classify 42
```

