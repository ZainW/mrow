import Foundation

/// The GraphQL GitLab is asked. One query fetches the whole queue: every merge request where
/// I'm a reviewer, my review state on each, and the activity notes that say when I was asked.
enum Queries {
  static let mergeRequestFields = """
    id iid title webUrl createdAt updatedAt mergedAt closedAt state draft
    sourceBranch targetBranch detailedMergeStatus userNotesCount description
    project { fullPath name }
    author { username name avatarUrl }
    reviewers { nodes { username name avatarUrl mergeRequestInteraction { reviewState approved } } }
    approvedBy { nodes { username } }
    diffStatsSummary { additions deletions fileCount }
    headPipeline { status }
    labels { nodes { title color textColor } }
    notes(filter: ONLY_ACTIVITY, last: 100) {
      pageInfo { hasPreviousPage startCursor }
      nodes { body createdAt author { username name avatarUrl } }
    }
    """

  static let queue = """
    query($state: MergeRequestState!, $after: String, $updatedAfter: Time) {
      currentUser {
        username name
        reviewRequestedMergeRequests(
          state: $state, first: 40, after: $after, updatedAfter: $updatedAfter, sort: UPDATED_DESC
        ) {
          pageInfo { hasNextPage endCursor }
          nodes { \(mergeRequestFields) }
        }
      }
    }
    """

  /// Older activity for one merge request, when its newest 100 notes didn't include my request.
  static let moreNotes = """
    query($path: ID!, $iid: String!, $before: String) {
      project(fullPath: $path) {
        mergeRequest(iid: $iid) {
          notes(filter: ONLY_ACTIVITY, last: 100, before: $before) {
            pageInfo { hasPreviousPage startCursor }
            nodes { body createdAt author { username name avatarUrl } }
          }
        }
      }
    }
    """
}
