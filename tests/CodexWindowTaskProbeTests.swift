import Foundation

@main
struct CodexWindowTaskProbeTests {
  static func main() {
    let link = "codex://threads/01a00000-0000-0000-0000-000000000001"
    precondition(validateCopiedTaskLink(link, beforeChange: 10, afterChange: 11) != nil)
    precondition(validateCopiedTaskLink(link, beforeChange: 10, afterChange: 12) == nil)
    precondition(validateCopiedTaskLink(link, beforeChange: 10, afterChange: 10) == nil)
    precondition(validateCopiedTaskLink("codex://threads/not-a-task", beforeChange: 10, afterChange: 11) == nil)
    precondition(validateCopiedTaskLink("\(link)?window=1", beforeChange: 10, afterChange: 11) == nil)
    precondition(validateCopiedTaskLink("codex://threads/01A00000-0000-0000-0000-000000000001", beforeChange: 10, afterChange: 11) == nil)
    precondition(validateCopiedTaskLink("https://example.com/threads/01a00000-0000-0000-0000-000000000001", beforeChange: 10, afterChange: 11) == nil)
    print("Selected-task probe validation passed")
  }
}
