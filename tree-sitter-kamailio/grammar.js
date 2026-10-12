/**
 * Tree-sitter grammar for the Kamailio configuration file language
 * (kamailio.cfg).  Core coverage: preprocessor directives, globals,
 * loadmodule/modparam, route-family blocks, control flow, calls,
 * pseudo-variables and transformations.
 */

module.exports = grammar({
  name: 'kamailio',

  // Block comments NEST: cfg.lex counts `comment_nest` up on every
  // `/*` inside a comment and leaves it only back at zero, which no
  // regular token can express — src/scanner.c matches them.
  externals: $ => [$.block_comment],

  // A directive is read by the lexer wherever it stands — between an
  // `if` body and its `else`, inside an expression list — so it is an
  // extra, not a statement.
  extras: $ => [/\s/, $.comment, $.block_comment, $.preproc],

  word: $ => $.identifier,

  rules: {
    source_file: $ => repeat($._top_level),

    _top_level: $ => choice(
      // cfg.y: SEMICOLON is a null statement at the top level too
      $.empty_statement,
      $.include,
      $.loadmodule,
      $.modparam,
      $.cfgengine,
      $.flags_declaration,
      $.socket_definition,
      $.custom_variable,
      $.global_assignment,
      $.route_definition,
    ),

    // #!define NAME value, #!ifdef/#!else/#!endif, #!subst*,
    // #!trydef/... — PREP_START ("#!" or "!!") followed by a directive
    // keyword and what cfg.lex's rule for it takes: `{EAT_ABLE}+` after
    // the define and conditional keywords, nothing but blanks after
    // else/endif.  One line with backslash continuations (#!define
    // bodies may span lines).  Anything else after "#!" is a # comment
    // and "!!" is two ! operators: `if (!!$var(x))` is code.  The
    // config-mode lines (#!KAMAILIO, #!SER, ...) are `{COM_LINE}!` rules
    // of their own.  Must outrank the # comment and the ! operator.
    preproc: _ => token(prec(2, choice(
      seq(
        choice('#!', '!!'),
        choice(
          seq(
            choice('define', 'def', 'trydefine', 'trydef', 'redefine', 'redef',
              'defexp', 'defexps', 'ifdef', 'ifndef', 'ifexp'),
            /[ \t\r]/,
            /(\\\r?\n|[^\n])*/,
          ),
          seq(choice('else', 'endif'), /[ \t\r]*/),
          seq(
            choice('subst', 'substdef', 'substdefs', 'defenv', 'defenvs',
              'trydefenv', 'trydefenvs'),
            /(\\\r?\n|[^\n])*/,
          ),
        ),
      ),
      seq('#!', choice('KAMAILIO', 'OPENSER', 'SER', 'MAXCOMPAT', 'ALL'), /[ \t\r]*/),
    ))),

    // line comments are "#" and "//" (cfg.lex COM_LINE)
    comment: _ => token(choice(seq('#', /[^\n]*/), seq('//', /[^\n]*/))),

    // The `#!`/`!!` prefix is honoured by the real parser — an
    // include written that way IS read — but `preproc` is a
    // whole-line token that would otherwise swallow it, so the
    // keyword needs a token of its own outranking it.  Without this
    // the grammar and the server's scanner disagree about what an
    // include is, and only one of them is right.
    include: $ => seq(
      token(prec(3, seq(
        optional(choice('#!', '!!')),
        choice('include_file', 'import_file'),
      ))),
      $.string,
    ),

    // cfg.y: LOADMODULE/LOADMODULEX take STRING, (STRING) or
    // (STRING, STRING)
    loadmodule: $ => seq(
      choice('loadmodule', 'loadmodulex', 'loadpath'),
      choice($.string, seq('(', $.string, optional(seq(',', $.string)), ')')),
    ),

    modparam: $ => seq(
      choice('modparam', 'modparamx'),
      '(', $.string, ',', $.string, ',', $._expression, ')',
    ),

    cfgengine: $ => seq('cfgengine', $.string),

    // cfg.y flags_decl: `flags NAME[: N], ...` (FLAGS_DECL is "flags"
    // or "bool"; a name is an ID or a STRING), and avpflags_decl:
    // `avpflags NAME, ...` with no numbers
    flags_declaration: $ => seq(
      choice('flags', 'bool', 'avpflags'),
      $.flag_spec,
      repeat(seq(',', $.flag_spec)),
    ),
    flag_spec: $ => seq(
      field('name', choice($.identifier, $.string)),
      optional(seq(':', $.number)),
    ),

    // cfg.y SOCKET EQUAL LBRACE socket_lattrs RBRACE
    socket_definition: $ => seq(
      'socket', '=', '{',
      repeat(choice(';', seq($.socket_attribute, optional(';')))),
      '}',
    ),
    socket_attribute: $ => seq(
      field('name', choice('bind', 'advertise', 'ADVERTISE', 'name', 'NAME',
        'agname', 'AGNAME', 'workers', 'virtual', 'vrf')),
      '=',
      field('value', choice($.socket_value, $.ip_address, $.host_value, $.string, $.number)),
    ),

    // cfg.y cfg_var: `group.name = NUMBER|STRING [desc STRING]`, also
    // `group[N].name = ...` (CFG_DESCRIPTION: description/descr/desc)
    custom_variable: $ => seq(
      field('name', $.custom_variable_name),
      '=',
      field('value', choice($.number, $.string)),
      optional(seq(choice('description', 'descr', 'desc'), $.string)),
    ),

    global_assignment: $ => seq(
      field('name', $.identifier), '=',
      field('value', choice(
        seq(repeat1($._listen_address), repeat($.listen_modifier)),
        $._expression,
      )),
    ),

    // `group.name` or `group[N].name`, one token so it cannot be read
    // as a host name
    custom_variable_name: _ => token(/[A-Za-z_][A-Za-z0-9_]*(\[[0-9]+\])?\.[A-Za-z_][A-Za-z0-9_]*/),

    // a bare address after `=` is a listen address, not an expression
    _listen_address: $ => prec(1, choice($.socket_value, $.ip_address, $.host_value)),

    // after a listen address list (cfg.y LISTEN EQUAL id_lst ...)
    listen_modifier: $ => choice(
      seq(choice('advertise', 'ADVERTISE'), $._listen_address),
      seq(choice('name', 'NAME'), $.string),
      'virtual',
    ),

    // raw socket/listen values: udp:127.0.0.1:5060, tls:eth0:5061,
    // udp:*:5060, LPROTO:LADDR:5060 (defines, substituted before the
    // parser reads them), 127.0.0.1:5060, [::1]:5060
    // outranks ip_address, which would otherwise take the address and
    // leave `:port` behind
    // A socket is also a call argument (`force_send_socket(udp:...)`),
    // so it stops at `,` and `)`.
    socket_value: _ => token(prec(3, choice(
      /[A-Za-z_][A-Za-z0-9_+.-]*:[A-Za-z0-9_.*\[\]:-]+/,
      /[0-9]{1,3}(\.[0-9]{1,3}){3}:[A-Za-z0-9_.*:-]+/,
      /\[[0-9A-Fa-f:.]+\](:[A-Za-z0-9_.*:-]+)?/,
    ))),

    // bare host/domain names: alias=sip.example.com, and in a
    // comparison (`src_ip==backup.example.com`); outranks identifier,
    // which would otherwise take the first label alone
    host_value: _ => token(prec(1, /[A-Za-z0-9][A-Za-z0-9_-]*(\.[A-Za-z0-9_-]+)+/)),

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
      // the lexer splices an include wherever it stands (accepted
      // inside a route block by kamailio -c 6.1.4)
      $.include,
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
      optional(choice($._expression, seq('(', ')'))),
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
      $.cast_expression,
      $.defined_expression,
      $.pseudo_variable,
      $.select,
      $.core_value,
      $.socket_value,
      $.host_value,
      $.string,
      $.string_concatenation,
      $.ip_address,
      $.number,
      $.identifier,
    ),

    // cfg.lex STR_BETWEEN: adjacent strings, across lines too, are one
    string_concatenation: $ => prec.left(seq($.string, repeat1($.string))),

    // cfg.lex INTCAST / STRCAST are single tokens
    cast_expression: $ => prec(14, seq(choice('(int)', '(str)'), $._expression)),

    defined_expression: $ => prec(13, seq('defined', $._expression)),

    // cfg.y select_id: @name[.name]... with [N] or ["s"] indexes
    select: _ => token(seq(
      '@',
      /[A-Za-z_][A-Za-z0-9_]*(\[[^\]\n]*\])?(\.[A-Za-z_][A-Za-z0-9_]*(\[[^\]\n]*\])?)*/,
    )),

    // cfg.lex MSGLEN, URIHOST, URIPORT: single tokens with a colon
    core_value: _ => token(prec(4, choice('msg:len', 'uri:host', 'uri:port'))),

    // cfg.y ipnet / ip: an IPv4 address (optionally /mask or /ip), or
    // an IPv6 address (cfg.lex IPV6ADDR)
    ip_address: _ => token(prec(2, choice(
      /[0-9]{1,3}(\.[0-9]{1,3}){3}(\/[0-9]{1,3}(\.[0-9]{1,3}){0,3})?/,
      /([0-9A-Fa-f]{1,4}:){1,7}(:[0-9A-Fa-f]{1,4}){1,7}|([0-9A-Fa-f]{1,4}:){7}[0-9A-Fa-f]{1,4}|:(:[0-9A-Fa-f]{1,4}){1,7}|([0-9A-Fa-f]{1,4}:){1,7}:|::/,
    ))),

    parenthesized_expression: $ => seq('(', $._expression, ')'),

    call_expression: $ => prec(2, seq(
      field('function', $.identifier),
      '(', optional(seq($._argument, repeat(seq(',', $._argument)))), ')',
    )),

    _argument: $ => $._expression,

    // operator set and precedence per cfg.lex / cfg.y: word forms
    // and/or/not, each bitwise operator on its own level, the string
    // and integer comparisons eq/ne/ieq/ine, modulo is the KEYWORD
    // `mod` (no `%`); there is no `!~`
    binary_expression: $ => {
      const table = [
        ['||', 1], ['or', 1], ['&&', 2], ['and', 2],
        ['|', 3], ['&', 4], ['^', 5], ['<<', 6], ['>>', 7],
        ['==', 8], ['!=', 8], ['=~', 8], ['eq', 8], ['ne', 8], ['ieq', 8], ['ine', 8],
        ['<', 9], ['>', 9], ['<=', 9], ['>=', 9],
        ['+', 10], ['-', 10],
        ['*', 11], ['/', 11], ['mod', 11],
      ];
      return choice(...table.map(([op, p]) =>
        prec.left(p, seq($._expression, op, $._expression))));
    },

    // cfg.y: NOT, UNARY (minus) and BIN_NOT share the level above `*`
    unary_expression: $ => prec(12, seq(choice('!', 'not', '-', '~'), $._expression)),

    // $ru, $var(x), $avp(name), $sht(t=>key), $(ru{s.len}),
    // $(avp(gw)[$(var(i){s.int})]) — cfg.lex PVAR_P counts nesting
    // without limit; two inner levels are matched here
    pseudo_variable: _ => token(seq(
      '$', optional('$'),
      choice(
        // the last return code (used by Kamailio's own ims/scscf config)
        '?',
        seq(
          /[A-Za-z_][A-Za-z0-9_.]*/,
          optional(seq('(', /(?:[^()\n]|\((?:[^()\n]|\([^()\n]*\))*\))*/, ')')),
        ),
        seq('(', /(?:[^()\n]|\((?:[^()\n]|\([^()\n]*\))*\))+/, ')'),
      ),
    )),

    // double-quoted strings escape with backslash; single-quoted
    // strings (cfg.lex STRING2) have NO escapes and may span lines
    // ... and double-quoted ones may span lines too (cfg.lex STRING1
    // takes {CR})
    string: _ => token(choice(
      seq('"', repeat(choice(/[^"\\]/, /\\(.|\n)/)), '"'),
      seq("'", /[^']*/, "'"),
    )),

    number: _ => /0[xX][0-9a-fA-F]+|[0-9]+/,

    identifier: _ => /[A-Za-z_][A-Za-z0-9_]*/,
  },
});
