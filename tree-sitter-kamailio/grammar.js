/**
 * Tree-sitter grammar for the Kamailio configuration file language
 * (kamailio.cfg).  Core coverage: preprocessor directives, globals,
 * loadmodule/modparam, route-family blocks, control flow, calls,
 * pseudo-variables and transformations.
 */

module.exports = grammar({
  name: 'kamailio',

  extras: $ => [/\s/, $.comment, $.block_comment],

  word: $ => $.identifier,

  rules: {
    source_file: $ => repeat($._top_level),

    _top_level: $ => choice(
      $.preproc,
      $.include,
      $.loadmodule,
      $.modparam,
      $.global_assignment,
      $.route_definition,
    ),

    // #!KAMAILIO, #!define NAME value, #!ifdef/#!else/#!endif,
    // #!subst*/#!trydef/... — PREP_START is "#!" or "!!" (cfg.lex),
    // one line with backslash continuations (#!define bodies may span
    // lines). Must outrank the # comment and the ! operator.
    preproc: _ => token(prec(2, seq(choice('#!', '!!'), /(\\\r?\n|[^\n])*/))),

    // line comments are "#" and "//" (cfg.lex COM_LINE)
    comment: _ => token(choice(seq('#', /[^\n]*/), seq('//', /[^\n]*/))),
    block_comment: _ => token(seq('/*', /[^*]*\*+([^/*][^*]*\*+)*/, '/')),

    include: $ => seq(choice('include_file', 'import_file'), $.string),

    loadmodule: $ => seq(choice('loadmodule', 'loadpath'), $.string),

    modparam: $ => seq(
      'modparam', '(', $.string, ',', $.string, ',', $._expression, ')',
    ),

    global_assignment: $ => seq(
      field('name', $.identifier), '=',
      field('value', choice($.socket_value, $.host_value, $._expression)),
    ),

    // raw socket/listen values: udp:127.0.0.1:5060, tls:eth0:5061,
    // udp:*:5060 — scheme-prefixed, unquoted
    socket_value: _ => token(prec(1, /[A-Za-z][A-Za-z0-9+.-]*:[^\s;#]+/)),

    // bare host/domain global values: alias=sip.example.com
    host_value: _ => token(prec(-1, /[A-Za-z0-9][A-Za-z0-9_-]*(\.[A-Za-z0-9_-]+)+/)),

    // request_route/reply_route take no name (verified: bracketed
    // forms are rejected by the parser); the other kinds may be named
    route_definition: $ => choice(
      seq(field('kind', alias($.unnamed_route_kind, $.route_kind)), field('body', $.block)),
      seq(
        field('kind', alias($.named_route_kind, $.route_kind)),
        optional(seq('[', field('name', choice($.event_name, $.identifier, $.number, $.string)), ']')),
        field('body', $.block),
      ),
    ),

    unnamed_route_kind: _ => choice('request_route', 'reply_route'),

    named_route_kind: _ => choice(
      'onreply_route', 'failure_route', 'branch_route', 'event_route',
      'onsend_route', 'route',
    ),

    // event_route names carry module:event and dashes:
    // event_route[htable:mod-init], event_route[tm:local-request]
    event_name: _ => token(/[A-Za-z_][A-Za-z0-9_.-]*:[A-Za-z0-9_.:-]+/),

    block: $ => seq('{', repeat($._statement), '}'),

    _statement: $ => choice(
      $.preproc,
      $.empty_statement,
      $.if_statement,
      $.while_statement,
      $.switch_statement,
      $.block,
      $.assignment_statement,
      $.expression_statement,
      $.keyword_statement,
      $.break_statement,
    ),

    // old documented style closes blocks with `};`
    empty_statement: _ => ';',

    if_statement: $ => prec.right(seq(
      'if', '(', $._expression, ')', $._statement,
      optional(seq('else', $._statement)),
    )),

    while_statement: $ => seq('while', '(', $._expression, ')', $._statement),

    switch_statement: $ => seq(
      'switch', '(', $._expression, ')',
      '{', repeat(choice($.case_clause, $.default_clause)), '}',
    ),
    case_clause: $ => seq('case', $._expression, ':', repeat($._statement)),
    default_clause: $ => seq('default', ':', repeat($._statement)),

    // return/exit/drop take an optional expression, parenthesized or
    // bare (`return 1;` and `return (1);` are both legal); break is
    // bare only (cfg.y BREAK carries no argument)
    keyword_statement: $ => seq(
      choice('exit', 'drop', 'return'),
      optional($._expression),
      ';',
    ),

    break_statement: _ => seq('break', ';'),

    // plain assignment only: ADDEQ is dead grammar in cfg.y and the
    // binary rejects `$var(x) += 1;`
    assignment_statement: $ => seq(
      field('target', $.pseudo_variable),
      '=',
      field('value', $._expression),
      ';',
    ),

    expression_statement: $ => seq($._expression, ';'),

    _expression: $ => choice(
      $.call_expression,
      $.binary_expression,
      $.unary_expression,
      $.parenthesized_expression,
      $.pseudo_variable,
      $.string,
      $.number,
      $.identifier,
    ),

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    call_expression: $ => prec(2, seq(
      field('function', $.identifier),
      '(', optional(seq($._argument, repeat(seq(',', $._argument)))), ')',
    )),

    _argument: $ => $._expression,

    // operator set per cfg.lex: word forms and/or/not, bitwise | &,
    // modulo is the KEYWORD `mod` (no `%`); there is no `!~`
    binary_expression: $ => {
      const table = [
        ['||', 1], ['or', 1], ['&&', 2], ['and', 2],
        ['|', 3], ['&', 4],
        ['==', 5], ['!=', 5], ['=~', 5],
        ['<', 6], ['>', 6], ['<=', 6], ['>=', 6],
        ['+', 7], ['-', 7],
        ['*', 8], ['/', 8], ['mod', 8],
      ];
      return choice(...table.map(([op, p]) =>
        prec.left(p, seq($._expression, op, $._expression))));
    },

    unary_expression: $ => prec(9, seq(choice('!', 'not', '-'), $._expression)),

    // $ru, $var(x), $avp(name), $sht(t=>key), $(ru{s.len}),
    // $(avp(gw)[$var(i)]) — one nesting level inside the parens
    pseudo_variable: _ => token(seq(
      '$', optional('$'),
      choice(
        seq(
          /[A-Za-z_][A-Za-z0-9_.]*/,
          optional(seq('(', /(?:[^()\n]|\([^()\n]*\))*/, ')')),
        ),
        seq('(', /(?:[^()\n]|\([^()\n]*\))+/, ')'),
      ),
    )),

    string: _ => token(seq('"', repeat(choice(/[^"\\\n]/, /\\./)), '"')),

    number: _ => /0[xX][0-9a-fA-F]+|[0-9]+/,

    identifier: _ => /[A-Za-z_][A-Za-z0-9_]*/,
  },
});
