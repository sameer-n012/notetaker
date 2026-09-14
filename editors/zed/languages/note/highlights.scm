; Block header: `label(args) #id {`
(block label: (label) @function)
(argument) @string
(block_id) @label

; Fences: ```lang ... ``` and $$$ ... $$$
"```" @punctuation.special
"$$$" @punctuation.special
(language) @attribute
(fence_line) @markup.raw

; Inline constructs within running text
(math_inline) @markup.math
(code_inline) @markup.raw.inline
(reference) @markup.link

; Punctuation
["{" "}"] @punctuation.bracket
["(" ")"] @punctuation.bracket
"," @punctuation.delimiter
