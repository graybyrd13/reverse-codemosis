# terraform 0.1.0

> Explanations for Terraform: public buckets, SSH and RDP open to the internet, secrets in code.

**Languages:** hcl · **Engine:** ^0.1 · **License:** MIT OR Apache-2.0 · **Home:** <https://github.com/graybyrd13/reverse-codemosis>

| Rule | Category | What it does | Proof |
|---|---|---|---|
| [`hardcoded-secret`](#hardcoded-secret) Secret written in the configuration | hazard | explains | 1 ex. · 3 counter |
| [`open-admin-port`](#open-admin-port) SSH or RDP open to the internet | hazard | explains | 1 ex. · 2 counter |
| [`public-bucket-acl`](#public-bucket-acl) Bucket readable by anyone | hazard | explains | 1 ex. · 1 counter |

## hardcoded-secret

**Secret written in the configuration** · `terraform/hardcoded-secret`

category: hazard · action: explains · CWE-798 · tags: security, secrets

A password, token or key written into Terraform code is readable by everyone with the repository, and its
history keeps it after it is removed. Pass it in as a variable marked `sensitive`, or read it from a secret
manager (`aws_secretsmanager_secret_version`, Vault, SOPS). Treat this one as compromised: rotate it.
Nothing is changed automatically.

**Shape**

```scheme
(attribute
  (identifier) @key
  (expression (literal_value (string_lit) @value))) @attr
```

**Facts that must hold**

- `matches(text(@key), '(?i)^(password|master_password|admin_password|secret|secret_key|client_secret|access_key|secret_access_key|token|api_key|api_token|private_key)$')`
- `len(text(@value)) > 2`

**Example: 1**

```text
resource "aws_db_instance" "main" {
  username = "admin"
  password = "correct-horse-battery"
}
```

**Counterexample: not a secret** (no finding)

```text
resource "aws_db_instance" "main" {
  username = "admin"
}
```

**Counterexample: an empty value** (no finding)

```text
resource "aws_db_instance" "main" {
  password = ""
}
```

**Counterexample: from a variable** (no finding)

```text
resource "aws_db_instance" "main" {
  password = var.db_password
}
```


## open-admin-port

**SSH or RDP open to the internet** · `terraform/open-admin-port`

category: hazard · action: explains · CWE-284 · tags: security, aws

`0.0.0.0/0` (or `::/0`) on port 22 (SSH) or 3389 (RDP) exposes the machine's login to the whole internet,
where it is scanned within minutes. Restrict `cidr_blocks` to the addresses that administer it, or reach it
through a bastion, a VPN or a session manager. Nothing is changed automatically: only you know which
addresses should reach it.

**Shape**

```scheme
(block
  (identifier) @kind
  (body) @body
  (#eq? @kind "ingress")) @block
```

**Facts that must hold**

- `matches(text(@body), '(?m)^\\s*(from_port|to_port)\\s*=\\s*(22|3389)\\s*$')`
- `matches(text(@body), '"(0\\.0\\.0\\.0/0|::/0)"')`

**Example: 1**

```text
resource "aws_security_group" "admin" {
  ingress {
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
}
```

**Counterexample: a web port open to everyone is the point of a web server** (no finding)

```text
resource "aws_security_group" "web" {
  ingress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }
}
```

**Counterexample: SSH from the office network** (no finding)

```text
resource "aws_security_group" "admin" {
  ingress {
    from_port   = 22
    to_port     = 22
    protocol    = "tcp"
    cidr_blocks = ["203.0.113.0/24"]
  }
}
```


## public-bucket-acl

**Bucket readable by anyone** · `terraform/public-bucket-acl`

category: hazard · action: explains · CWE-732 · tags: security, aws

The canned ACLs `public-read` and `public-read-write` grant every anonymous user access to the bucket's
objects. Most leaked data on S3 started this way. Keep buckets private and serve public files through a
CDN or a bucket policy scoped to them; if this bucket really is public, say so with a suppression and its
reason. Nothing is changed automatically: whether data is public is a decision.

**Shape**

```scheme
(attribute
  (identifier) @key
  (expression (literal_value (string_lit (template_literal) @value)))
  (#eq? @key "acl")
  (#match? @value "^public-read(-write)?$")) @attr
```

**Example: 1**

```text
resource "aws_s3_bucket_acl" "reports" {
  bucket = aws_s3_bucket.reports.id
  acl    = "public-read"
}
```

**Counterexample: 1** (no finding)

```text
resource "aws_s3_bucket_acl" "reports" {
  bucket = aws_s3_bucket.reports.id
  acl    = "private"
}
```

