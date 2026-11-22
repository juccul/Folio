# Folio 0.2.5

- **Inline Auto Math Solver (optional, Settings → Recognition)**: draw `=` after a handwritten expression (such as `4+4=`, `\sqrt{3}=`, `Tan(\frac{\pi}{4})=`, `\frac{4}{\frac{4}{4}}=`, or `100^{10}=`) to automatically synthesize handwritten answer ink matching your pen color, height, slant, and pressure.
  - **Natural write-on animation**: strokes appear sequentially with a slight overlapping stagger in golden-yellow ink and smoothly fade to your pen color once finished.
  - **Detected-LaTeX hover popup**: hover over any auto-completed answer to see the rendered LaTeX of what was recognized to produce it, along with any page variable or distribution context that contributed.
  - **Page variables & simple algebra**: resolves handwritten variable assignments (`y = 3`, `x = y^2 + 4`), linear equations (`2x + 4 = 10` → `x = 3`), case-insensitive single-letter variable fallbacks, and polynomial simplification (`2x + 3x = 5x`, `(x + 1)^2 = x^2 + 2x + 1`).
  - **Robust 2D equation detection**: isolates tightly stacked rows while preserving character cap/crossbars (`T` in `Tan(...)`, `\pi`, `t`, `5`), bowed/nested fraction bars, multi-character subscripts/superscripts, compact `=` signs, and parenthesized `=` inside `P(X = k) =`.
- **Built-in Statistics Table Solver** (inline Auto Math & Math Panel): pure-Rust exact solver covering all standard university statistics reference tables:
  - Standard & general Normal ($\Phi(z)$, $\varphi(z)$, $\Phi^{-1}(p)$, $z_\alpha$, $P(Z \le z)$, $X \sim N(\mu, \sigma^2)$)
  - Student's $t$ ($t_{\text{df}, \alpha}$, $t_{\alpha, \text{df}}$, $\text{df} = \infty$, $P(T \ge t)$)
  - Chi-square ($\chi^2_{\text{df}, \alpha}$, $P(\chi^2 \ge c)$)
  - $F$-distribution ($F_\alpha(f_1, f_2)$ for any $\alpha$, including $f_1 = \infty$ or $f_2 = \infty$)
  - Shapiro-Wilk normality test (exact $n = 2..50$ weights `sw_a(n, i)`, exact $n = 3..50$ critical values $W_{n, \alpha}$, direct sample $W(x_1, \dots, x_n)$ statistic, and Royston AS R94 fallback)
  - Binomial $B(n, p)$ and Poisson $\text{Pois}(\mu)$ cumulative probabilities, PMFs, $E(X)$, $\operatorname{Var}(X)$, and page `X \sim ...` declarations.
- **Improved offline handwriting & math recognition**:
  - **Handwriting v2**: CTC lexicon/edit-distance word rescoring reduces 1–2 letter typos while preserving non-dictionary words, numbers, and punctuation.
  - **Math v4**: eliminates `<unk>` decoding artifacts and improves handwritten letters, trig/log functions (`Sin`, `Tan`, `log`), `0` vs `\circ`, `\pi`, and multi-digit exponents (`77.20%` validation exact match, `4.69%` CER).
- **Smoother dot-grid scrolling**: replaces per-frame dot-grid vector tessellation + 4× MSAA with a cached repeating tile texture and retains adjacent pages in cache during vertical scrolling.

Windows x64 setup EXE and portable ZIP, Linux x86_64 Flatpak and desktop archive, checksums and complete corresponding source are published together with the bundled `recognition/` models. Updates remain signed and user-initiated.

Database schema remains 6 and document format remains 4. See [UPDATES.md](UPDATES.md) for installation and recovery.
