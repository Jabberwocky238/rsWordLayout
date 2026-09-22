
set theDoc to document "vertical-precision-capture-20260922.docx"
set acc to {}
set n to count of paragraphs of text object of theDoc
repeat with i from 1 to n
    set r to text object of (paragraph i of text object of theDoc)
    set s to start of content of r
    set e to end of content of r
    set end of acc to ((s as text) & "," & (e as text))
end repeat
set AppleScript's text item delimiters to linefeed
set out to acc as text
set AppleScript's text item delimiters to ""
return out
