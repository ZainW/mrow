import SwiftUI

/// The composer's model button ("Sonnet 5.5 Medium ⌄"), opening the picker above it.
struct ModelButton: View {
  var chat: Chat?
  @Binding var open: Bool
  @ObservedObject private var ask = AskStore.shared

  var body: some View {
    ComposerChip(
      symbol: nil,
      title: ask.chosenModel?.shortTitle ?? AgentModel(id: ask.model, name: "").shortTitle,
      trailing: ask.effort(for: ask.model)?.name,
      tint: .mrowSecondary,
      isOn: open
    ) {
      open.toggle()
    }
    .fixedSize()
    .help("Choose the model and effort  ⌥⌘L")
    .accessibilityLabel("Model")
    .popover(isPresented: $open, arrowEdge: .top) {
      ModelPicker(chat: chat) { open = false }
    }
  }
}

/// Cursor's models the way Claude offers its own: a short list of the newest of a few families,
/// each with a line on when it suits, "More models" for the rest (searchable, by maker), and the
/// chosen model's effort on a slider at the foot.
///
/// Keys: ↑↓ move, ↩ picks and closes, ←→ change the effort, typing searches every model, esc
/// goes back or closes. A click picks a model and leaves the picker open for its effort.
struct ModelPicker: View {
  var chat: Chat?
  var done: () -> Void

  @ObservedObject private var ask = AskStore.shared
  @State private var page = Page.featured
  @State private var query = ""
  @State private var highlighted: String?
  /// Where the full list scrolls to: set by the keyboard, not by hovering, so the list never
  /// moves under the pointer.
  @State private var scrollTarget: String?
  @FocusState private var focus: Focus?
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  private enum Page { case featured, all }
  private enum Focus { case list, search }
  private static let moreID = "more"

  var body: some View {
    ZStack(alignment: .top) {
      switch page {
      case .featured: featuredPage.transition(slide(.leading))
      case .all: allPage.transition(slide(.trailing))
      }
    }
    .frame(width: 304)
    .clipped()
    .background(TakesKey())
    .task {
      await ask.loadModels()
      await ask.describe(ask.model)
    }
  }

  private func slide(_ edge: Edge) -> AnyTransition {
    reduceMotion ? .opacity : .move(edge: edge).combined(with: .opacity)
  }

  private func go(_ next: Page) {
    withAnimation(reduceMotion ? nil : .easeOut(duration: 0.18)) { page = next }
    highlighted = next == .featured ? featured.first { $0.id == ask.model }?.id : allRows.first?.id
    scrollTarget = highlighted
    focus = next == .featured ? .list : .search
  }

  // MARK: Featured

  /// The newest of each featured family, then the chosen model and recent ones when they aren't
  /// among them, so they stay in reach.
  private var featured: [AgentModel] {
    var list = AgentModel.featured(in: ask.models)
    for id in [ask.model] + ask.recentModels {
      if let model = ask.models.first(where: { $0.id == id }), !list.contains(model) { list.append(model) }
    }
    return Array(list.prefix(8))
  }

  private var featuredPage: some View {
    VStack(spacing: 0) {
      if ask.models.isEmpty {
        loading.frame(height: 220)
      } else {
        VStack(spacing: 1) {
          ForEach(featured) { model in
            featuredRow(model)
          }
          moreRow
        }
        .padding(6)
      }
      Rectangle().fill(Color.mrowLine).frame(height: 1)
      effort
    }
    // The list takes keys itself: there's no search field on this page.
    .focusable()
    .focusEffectDisabled()
    .focused($focus, equals: .list)
    .onKeyPress(.downArrow) { move(1, in: featured.map(\.id) + [Self.moreID]) }
    .onKeyPress(.upArrow) { move(-1, in: featured.map(\.id) + [Self.moreID]) }
    .onKeyPress(.leftArrow) { stepEffort(-1) }
    .onKeyPress(.rightArrow) { stepEffort(1) }
    .onKeyPress(.return) {
      if highlighted == Self.moreID {
        go(.all)
      } else if let model = featured.first(where: { $0.id == highlighted }) {
        ask.choose(model: model, for: chat)
        done()
      }
      return .handled
    }
    .onKeyPress(.escape) {
      done()
      return .handled
    }
    .onKeyPress(characters: .alphanumerics, phases: .down) { press in
      query = press.characters
      go(.all)
      return .handled
    }
    .onAppear {
      focus = .list
      highlighted = featured.first { $0.id == ask.model }?.id ?? featured.first?.id
    }
  }

  private func featuredRow(_ model: AgentModel) -> some View {
    let chosen = model.id == ask.model
    let detail =
      model.blurb ?? (ask.recentModels.contains(model.id) ? "\(model.maker) · used recently" : model.maker)
    return Button {
      ask.choose(model: model, for: chat)
    } label: {
      HStack(spacing: 10) {
        VStack(alignment: .leading, spacing: 2) {
          Text(model.title).font(.mrowUI(13, .medium)).foregroundStyle(Color.mrowText)
          Text(detail).font(.mrowUI(11.5)).foregroundStyle(Color.mrowTertiary).lineLimit(1)
        }
        Spacer(minLength: 6)
        if chosen {
          Image(systemName: "checkmark")
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(Color.mrowAccent)
        }
      }
      .padding(.horizontal, 10)
      .padding(.vertical, 7)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(highlightFill(model.id), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .onHover { inside in if inside { highlighted = model.id } }
    .accessibilityAddTraits(chosen ? .isSelected : [])
  }

  private var moreRow: some View {
    Button {
      go(.all)
    } label: {
      HStack {
        Text("More models").font(.mrowUI(13, .medium)).foregroundStyle(Color.mrowText)
        Spacer()
        Text("\(ask.models.count)").font(.mrowUI(11.5)).monospacedDigit().foregroundStyle(Color.mrowTertiary)
        Image(systemName: "chevron.right")
          .font(.system(size: 10, weight: .semibold))
          .foregroundStyle(Color.mrowTertiary)
      }
      .padding(.horizontal, 10)
      .frame(height: 34)
      .background(highlightFill(Self.moreID), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .onHover { inside in if inside { highlighted = Self.moreID } }
  }

  private func highlightFill(_ id: String) -> Color {
    highlighted == id ? .mrowSelected : .clear
  }

  // MARK: Effort

  /// The chosen model's effort levels, as Cursor names them, on a slider.
  private var effort: some View {
    let options = ask.efforts[ask.model]
    let values = options?.values ?? []
    let current = values.firstIndex { $0.value == ask.effort(for: ask.model)?.value } ?? 0
    return VStack(alignment: .leading, spacing: 9) {
      HStack(alignment: .firstTextBaseline) {
        Text("Effort").font(.mrowUI(12.5, .medium)).foregroundStyle(Color.mrowText)
        Spacer()
        if !values.isEmpty {
          Text(values[current].name)
            .font(.mrowUI(12.5, .medium))
            .foregroundStyle(Color.mrowAccent)
            .contentTransition(.numericText())
        }
      }
      if !values.isEmpty {
        EffortSlider(labels: values.map(\.name), index: current) { index in
          ask.choose(effort: values[index].value, for: chat)
        }
        HStack {
          Text("Faster")
          Spacer()
          Text("More thorough")
        }
        .font(.mrowUI(11))
        .foregroundStyle(Color.mrowTertiary)
      } else if options != nil {
        Text("\(ask.chosenModel?.title ?? "This model") sets its own effort.")
          .font(.mrowUI(11.5))
          .foregroundStyle(Color.mrowTertiary)
      } else {
        HStack(spacing: 6) {
          ProgressView().controlSize(.mini)
          Text("Asking Cursor for its effort levels…")
        }
        .font(.mrowUI(11.5))
        .foregroundStyle(Color.mrowTertiary)
      }
    }
    .padding(.horizontal, 16)
    .padding(.top, 12)
    .padding(.bottom, 14)
    .help(values.isEmpty ? "" : "How long \(ask.chosenModel?.title ?? "the model") thinks before it answers  ←→")
  }

  private func stepEffort(_ step: Int) -> KeyPress.Result {
    guard let options = ask.efforts[ask.model]?.values, !options.isEmpty else { return .ignored }
    let current = ask.effort(for: ask.model)?.value
    let index = options.firstIndex { $0.value == current } ?? 0
    let next = options[min(max(index + step, 0), options.count - 1)]
    if next.value != current { ask.choose(effort: next.value, for: chat) }
    return .handled
  }

  // MARK: All models

  private struct Group: Identifiable {
    let id: String
    let models: [AgentModel]
  }

  private var groups: [Group] {
    let words = query.lowercased().split(separator: " ")
    let matching = ask.models.filter { model in
      let text = [model.title, model.maker, model.id].joined(separator: " ").lowercased()
      return words.allSatisfy { text.contains($0) }
    }
    return AgentModel.makers.compactMap { maker in
      let models = matching.filter { $0.maker == maker }
      return models.isEmpty ? nil : Group(id: maker, models: models)
    }
  }

  private var allRows: [AgentModel] { groups.flatMap(\.models) }

  private var allPage: some View {
    VStack(spacing: 0) {
      HStack(spacing: 6) {
        Button {
          query = ""
          go(.featured)
        } label: {
          Image(systemName: "chevron.left")
            .font(.system(size: 11.5, weight: .semibold))
            .foregroundStyle(Color.mrowSecondary)
            .frame(width: 24, height: 24)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .help("Back")
        Image(systemName: "magnifyingglass")
          .font(.system(size: 11.5, weight: .medium))
          .foregroundStyle(Color.mrowTertiary)
        TextField("Search \(ask.models.count) models", text: $query)
          .textFieldStyle(.plain)
          .font(.mrowUI(13))
          .focused($focus, equals: .search)
          .onSubmit {
            if let model = allRows.first(where: { $0.id == highlighted }) ?? allRows.first {
              ask.choose(model: model, for: chat)
              done()
            }
          }
          .onKeyPress(.downArrow) { move(1, in: allRows.map(\.id)) }
          .onKeyPress(.upArrow) { move(-1, in: allRows.map(\.id)) }
          .onKeyPress(.escape) {
            query = ""
            go(.featured)
            return .handled
          }
        if ask.loadingModels {
          ProgressView().controlSize(.mini)
        } else {
          ChromeButton(symbol: "arrow.clockwise", title: "Refresh the list", symbolSize: 10.5) {
            Task { await ask.loadModels(force: true) }
          }
          .frame(width: 24, height: 24)
        }
      }
      .padding(.leading, 8)
      .padding(.trailing, 8)
      .frame(height: 42)
      Rectangle().fill(Color.mrowLine).frame(height: 1)
      if allRows.isEmpty {
        Text(ask.models.isEmpty ? "No models yet" : "No models match “\(query)”")
          .font(.mrowUI(12.5))
          .foregroundStyle(Color.mrowTertiary)
          .frame(maxWidth: .infinity, maxHeight: .infinity)
      } else {
        allList
      }
    }
    .frame(height: 440)
    .onChange(of: query) { _, _ in
      highlighted = allRows.first?.id
      scrollTarget = highlighted
    }
  }

  private var allList: some View {
    ScrollViewReader { proxy in
      ScrollView {
        LazyVStack(alignment: .leading, spacing: 0) {
          ForEach(groups) { group in
            Text(group.id)
              .font(.mrowUI(11, .medium))
              .foregroundStyle(Color.mrowTertiary)
              .padding(.horizontal, 16)
              .padding(.top, 10)
              .padding(.bottom, 4)
            ForEach(group.models) { model in
              allRow(model).id(model.id)
            }
          }
        }
        .padding(.bottom, 6)
        .frame(maxWidth: .infinity, alignment: .leading)
      }
      .onChange(of: scrollTarget) { _, id in
        if let id { proxy.scrollTo(id) }
      }
    }
  }

  private func allRow(_ model: AgentModel) -> some View {
    let chosen = model.id == ask.model
    return Button {
      ask.choose(model: model, for: chat)
    } label: {
      HStack(spacing: 8) {
        Text(model.title).font(.mrowUI(12.5, .medium)).foregroundStyle(Color.mrowText)
        Spacer(minLength: 6)
        if chosen {
          Image(systemName: "checkmark")
            .font(.system(size: 10.5, weight: .bold))
            .foregroundStyle(Color.mrowAccent)
        }
      }
      .padding(.horizontal, 10)
      .frame(height: 30)
      .frame(maxWidth: .infinity, alignment: .leading)
      .background(highlightFill(model.id), in: RoundedRectangle(cornerRadius: 7, style: .continuous))
      .contentShape(Rectangle())
    }
    .buttonStyle(.plain)
    .padding(.horizontal, 6)
    .onHover { inside in if inside { highlighted = model.id } }
    .accessibilityAddTraits(chosen ? .isSelected : [])
  }

  @ViewBuilder private var loading: some View {
    VStack(spacing: 10) {
      if let error = ask.modelsError, !ask.loadingModels {
        Text(error)
          .font(.mrowUI(12))
          .foregroundStyle(Color.mrowRed)
          .multilineTextAlignment(.center)
        Button("Try Again") { Task { await ask.loadModels(force: true) } }
      } else {
        ProgressView().controlSize(.small)
        Text("Asking Cursor for its models…")
          .font(.mrowUI(12))
          .foregroundStyle(Color.mrowTertiary)
      }
    }
    .padding(20)
    .frame(maxWidth: .infinity)
  }

  private func move(_ step: Int, in ids: [String]) -> KeyPress.Result {
    guard !ids.isEmpty else { return .handled }
    let index = highlighted.flatMap { ids.firstIndex(of: $0) } ?? (step > 0 ? -1 : ids.count)
    highlighted = ids[min(max(index + step, 0), ids.count - 1)]
    scrollTarget = highlighted
    return .handled
  }
}

/// A slider that stops at each effort level: a track filled in clay up to the knob, a dot at
/// every stop. Drag it, click a stop, or use ←→.
struct EffortSlider: View {
  let labels: [String]
  let index: Int
  let choose: (Int) -> Void
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  private let knob: CGFloat = 16
  private let track: CGFloat = 6

  var body: some View {
    GeometryReader { geometry in
      let width = geometry.size.width
      let mid = geometry.size.height / 2
      let inset = knob / 2
      let span = width - 2 * inset
      let x = { (stop: Int) in inset + span * CGFloat(stop) / CGFloat(max(labels.count - 1, 1)) }
      ZStack(alignment: .topLeading) {
        Capsule()
          .fill(Color.mrowWell)
          .overlay(Capsule().strokeBorder(Color.mrowLine))
          .frame(width: span + track, height: track)
          .position(x: width / 2, y: mid)
        Capsule()
          .fill(Color.mrowAccent)
          .frame(width: x(index) - inset + track, height: track)
          .position(x: inset - track / 2 + (x(index) - inset + track) / 2, y: mid)
        ForEach(labels.indices, id: \.self) { stop in
          Circle()
            .fill(stop <= index ? Color.white.opacity(0.85) : Color.mrowStrongLine)
            .frame(width: 3, height: 3)
            .position(x: x(stop), y: mid)
        }
        // White in both appearances, like a system slider's knob, so it stands off the track.
        Circle()
          .fill(Color.white)
          .overlay(Circle().strokeBorder(Color.black.opacity(0.12)))
          .shadow(color: .black.opacity(0.2), radius: 2, y: 1)
          .frame(width: knob, height: knob)
          .position(x: x(index), y: mid)
      }
      .frame(width: width, height: geometry.size.height)
      .contentShape(Rectangle())
      .gesture(
        DragGesture(minimumDistance: 0)
          .onChanged { value in
            let stop = Int(((value.location.x - inset) / max(span, 1) * CGFloat(labels.count - 1)).rounded())
            let clamped = min(max(stop, 0), labels.count - 1)
            if clamped != index { choose(clamped) }
          }
      )
      .animation(reduceMotion ? nil : .snappy(duration: 0.18), value: index)
    }
    .frame(height: 20)
    .accessibilityElement()
    .accessibilityLabel("Effort")
    .accessibilityValue(labels.indices.contains(index) ? labels[index] : "")
    .accessibilityAdjustableAction { direction in
      switch direction {
      case .increment: if index < labels.count - 1 { choose(index + 1) }
      case .decrement: if index > 0 { choose(index - 1) }
      @unknown default: break
      }
    }
  }
}
