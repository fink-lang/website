---
template: home
---

```fink
# Pipes and partial application
1..10
| filter ? % 2 == 0
| map ? * 2
| [..?]
|= even_nums
```

```fink
# Pattern matching with destructuring
classify = fn match n:
  n > 0: 'positive'
  n < 0: 'negative'
  else:  'zero'
```

```fink
# Error handling — no exceptions
fn fetch_user id:
  user = try get_user id
  Ok user.name
```
