; Highlighting for kamailio.cfg.  Capture names are the common set
; Neovim, Helix and Zed all map to colours.

(comment) @comment
(block_comment) @comment
(preproc) @keyword.directive

(string) @string
(number) @number
(ip_address) @string.special
(socket_value) @string.special
(host_value) @string.special

(pseudo_variable) @variable.builtin
(select) @variable.builtin
(core_value) @variable.builtin
(global_assignment
  name: (identifier) @variable)
(custom_variable
  name: (custom_variable_name) @variable)
(flag_spec
  name: (identifier) @constant)

; route blocks and their names
(route_kind) @keyword
(route_definition
  name: (_) @function)

; `route(NAME)` calls a route block; every other call is a function
((call_expression
  function: (identifier) @keyword
  (identifier) @function)
  (#eq? @keyword "route"))
((call_expression
  function: (identifier) @function.call)
  (#not-eq? @function.call "route"))

; the directive keyword is part of the node; its path stays a string
(include) @keyword

[
  "loadmodule"
  "loadmodulex"
  "loadpath"
  "modparam"
  "modparamx"
  "cfgengine"
  "flags"
  "bool"
  "avpflags"
  "socket"
  "if"
  "else"
  "while"
  "switch"
  "case"
  "default"
  "exit"
  "drop"
  "return"
  "break"
  "defined"
  "advertise"
  "ADVERTISE"
  "name"
  "NAME"
  "virtual"
] @keyword

(socket_attribute
  name: _ @property)

[
  "=" "==" "!=" "=~" "<" ">" "<=" ">="
  "eq" "ne" "ieq" "ine"
  "||" "or" "&&" "and" "!" "not" "~"
  "|" "&" "^" "<<" ">>"
  "+" "-" "*" "/" "mod"
  "(int)" "(str)"
] @operator

["(" ")" "[" "]" "{" "}"] @punctuation.bracket
["," ";" ":"] @punctuation.delimiter
