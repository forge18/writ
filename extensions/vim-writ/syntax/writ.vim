" Vim syntax file for the Writ scripting language
if exists('b:current_syntax')
  finish
endif

" Keywords - control flow
syn keyword writControl       if else when while for in break continue return
syn keyword writControl       start yield

" Keywords - declarations
syn keyword writStorage       class func trait enum struct let var const

" Keywords - modifiers
syn keyword writModifier      public private static extends with set

" Keywords - import/export
syn keyword writImport        import export from

" Keywords - operators
syn keyword writOperator      is as

" Constants
syn keyword writBoolean       true false
syn keyword writNull          null
syn keyword writSelf          self

" Primitive types
syn keyword writType          int float string bool void
syn keyword writType          bigint uint ubigint bigfloat

" Generic types
syn keyword writGenericType   Array Dictionary Result Optional Success Error

" Numbers
syn match writFloat           '\<\d\+\.\d\+\>'
syn match writInteger         '\<\d\+\>'

" Strings
syn region writString         start='"' skip='\\"' end='"'
      \ contains=writInterpolationBlock,writInterpolationSimple,writEscape

" Multiline strings
syn region writMultiString    start='"""' end='"""'
      \ contains=writInterpolationBlock,writInterpolationSimple,writEscape

" String interpolation
syn region writInterpolationBlock matchgroup=writInterpolationDelim
      \ start='\${'  end='}'
      \ contained contains=TOP

syn match writInterpolationSimple '\$[a-zA-Z_][a-zA-Z0-9_]*'
      \ contained

" Escape sequences
syn match writEscape          '\\[nrt\\\"$0]' contained

" Comments
syn match writLineComment     '//.*$'
syn region writBlockComment   start='/\*' end='\*/'

" Operators
syn match writArrow           '->'
syn match writFatArrow        '=>'
syn match writSpread          '\.\.\.'
syn match writRangeInclusive  '\.\.='
syn match writRange           '\.\.'
syn match writNullCoalesce    '??'
syn match writSafeAccess      '?\.'
syn match writNamespace       '::'

" Highlighting links
hi def link writControl           Keyword
hi def link writStorage           StorageClass
hi def link writModifier          StorageClass
hi def link writImport            Include
hi def link writOperator          Operator
hi def link writBoolean           Boolean
hi def link writNull              Constant
hi def link writSelf              Identifier
hi def link writType              Type
hi def link writGenericType       Type
hi def link writFloat             Float
hi def link writInteger           Number
hi def link writString            String
hi def link writMultiString       String
hi def link writInterpolationBlock Special
hi def link writInterpolationSimple Special
hi def link writInterpolationDelim Delimiter
hi def link writEscape            SpecialChar
hi def link writLineComment       Comment
hi def link writBlockComment      Comment
hi def link writArrow             Operator
hi def link writFatArrow          Operator
hi def link writSpread            Operator
hi def link writRangeInclusive    Operator
hi def link writRange             Operator
hi def link writNullCoalesce      Operator
hi def link writSafeAccess        Operator
hi def link writNamespace         Operator

let b:current_syntax = 'writ'
