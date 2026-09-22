set d to document "vertical-precision-capture-20260922.docx"
return (POSIX path of ((full name of d) as alias)) & "|saved=" & (saved of d as text) & "|count=" & (count of documents as text)