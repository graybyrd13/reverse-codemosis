# MARK_COMMENT
$Greeting = "MARK_STRING"

function Get-Greeting {
    param([string]$Name)
    return "$Greeting, $Name"
}
