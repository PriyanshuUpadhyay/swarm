window.renderPreview = (text, isDiff, split, dark) => {
  document.documentElement.style.colorScheme = dark ? 'dark' : 'light';
  document.body.className = dark ? 'd2h-dark-color-scheme' : 'd2h-light-color-scheme';
  const target = document.getElementById('content');
  target.replaceChildren();
  if (isDiff && /^diff --git /m.test(text)) {
    const ui = new Diff2HtmlUI(target, text, {
      outputFormat: split ? 'side-by-side' : 'line-by-line',
      drawFileList: false,
      colorScheme: dark ? 'dark' : 'light',
      matching: 'none',
      diffMaxChanges: 10000,
      diffMaxLineLength: 20000,
      maxLineLengthHighlight: 2000,
      synchronisedScroll: true,
      highlight: true,
      fileContentToggle: false,
      stickyFileHeaders: true
    });
    ui.draw();
  } else {
    const pre = document.createElement('pre');
    pre.textContent = text || (isDiff ? 'No diff content.' : 'Empty file.');
    target.appendChild(pre);
  }
};
