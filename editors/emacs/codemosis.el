;;; codemosis.el --- Reverse Codemosis: proven corrections and questions  -*- lexical-binding: t; -*-

;; Version: 0.1.0
;; Package-Requires: ((emacs "29.1"))
;; URL: https://github.com/graybyrd13/reverse-codemosis
;; SPDX-License-Identifier: MIT OR Apache-2.0

;;; Commentary:

;; Reverse Codemosis finds where code falls short, applies only the
;; corrections it can prove, and asks you when a decision is yours.
;;
;; - With lsp-mode, `codemosis lsp' runs as an add-on next to your other
;;   language servers: findings, proven corrections as code actions,
;;   questions answered with code actions, hover.
;; - With eglot, which runs one server per buffer, `codemosis-eglot-setup'
;;   makes `codemosis lsp' the server for `codemosis-eglot-modes'.
;; - `codemosis-check' and `codemosis-check-changed' list findings in a
;;   compilation buffer, so `next-error' visits them.
;;
;;   (require 'codemosis)

;;; Code:

(require 'compile)

(defgroup codemosis nil
  "Reverse Codemosis."
  :group 'tools
  :prefix "codemosis-")

(defcustom codemosis-executable "codemosis"
  "The `codemosis' executable."
  :type 'string)

(defcustom codemosis-eglot-modes
  '(python-mode python-ts-mode js-mode js-ts-mode typescript-ts-mode tsx-ts-mode
    go-mode go-ts-mode rust-mode rust-ts-mode java-mode java-ts-mode c-mode c-ts-mode
    c++-mode c++-ts-mode csharp-mode csharp-ts-mode ruby-mode ruby-ts-mode php-mode
    sh-mode bash-ts-mode kotlin-mode scala-mode lua-mode swift-mode)
  "Major modes for which `codemosis-eglot-setup' registers the server."
  :type '(repeat symbol))

(defun codemosis--root ()
  "The project root: the nearest directory with codemosis.toml or .git."
  (or (locate-dominating-file default-directory "codemosis.toml")
      (locate-dominating-file default-directory ".git")
      default-directory))

;;;###autoload
(defun codemosis-check (&optional changed)
  "List Reverse Codemosis findings in a compilation buffer.
With prefix argument CHANGED, only files changed since the last commit."
  (interactive "P")
  (let ((default-directory (codemosis--root)))
    (compile (concat (shell-quote-argument codemosis-executable)
                     " check --format compact --fail-on never --color never"
                     (if changed " --changed" "")))))

;;;###autoload
(defun codemosis-check-changed ()
  "List findings in files changed since the last commit."
  (interactive)
  (codemosis-check t))

;;;###autoload
(defun codemosis-fix ()
  "Apply the proven corrections, verified by the build and tests."
  (interactive)
  (let ((default-directory (codemosis--root)))
    (compile (concat (shell-quote-argument codemosis-executable) " fix --color never") t)))

;;;###autoload
(defun codemosis-eglot-setup ()
  "Use `codemosis lsp' as the eglot server for `codemosis-eglot-modes'."
  (interactive)
  (require 'eglot)
  (add-to-list 'eglot-server-programs
               (cons codemosis-eglot-modes (list codemosis-executable "lsp"))))

(with-eval-after-load 'lsp-mode
  (when (fboundp 'lsp-register-client)
    (lsp-register-client
     (make-lsp-client
      :new-connection (lsp-stdio-connection (lambda () (list codemosis-executable "lsp")))
      :activation-fn (lambda (&rest _) t)
      :add-on? t
      :priority -1
      :server-id 'codemosis))))

(provide 'codemosis)
;;; codemosis.el ends here
