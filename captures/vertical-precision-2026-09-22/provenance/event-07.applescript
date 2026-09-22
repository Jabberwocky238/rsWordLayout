
set theDoc to document "vertical-precision-capture-20260922.docx"
set theRange to text object of theDoc
set eoc to end of content of theRange
set acc to {}
repeat with i from 0 to (eoc - 1)
    set r to create range theDoc start i end i
    set ln to (get range information r information type first character line number)
    set pg to (get range information r information type active end page number)
    set end of acc to ((i as text) & "," & (ln as text) & "," & (pg as text))
end repeat
set AppleScript's text item delimiters to linefeed
set out to acc as text
set AppleScript's text item delimiters to ""
return (eoc as text) & linefeed & "---" & linefeed & out
