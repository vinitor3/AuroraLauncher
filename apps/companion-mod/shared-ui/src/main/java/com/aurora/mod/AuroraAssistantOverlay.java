package com.aurora.mod;

import java.awt.BasicStroke;
import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.Font;
import java.awt.Graphics;
import java.awt.Graphics2D;
import java.awt.GraphicsEnvironment;
import java.awt.Image;
import java.awt.Rectangle;
import java.awt.RenderingHints;
import java.awt.Robot;
import java.awt.event.ActionEvent;
import java.awt.event.InputEvent;
import java.awt.event.KeyEvent;
import java.awt.image.BufferedImage;
import java.io.ByteArrayOutputStream;
import java.util.Base64;
import java.util.UUID;

import javax.imageio.ImageIO;
import javax.swing.AbstractAction;
import javax.swing.BorderFactory;
import javax.swing.JButton;
import javax.swing.JComponent;
import javax.swing.JLabel;
import javax.swing.JOptionPane;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JTextArea;
import javax.swing.JTextField;
import javax.swing.JWindow;
import javax.swing.KeyStroke;
import javax.swing.SwingConstants;
import javax.swing.SwingUtilities;
import javax.swing.Timer;

/** HUD leve do Assistente, executado no processo cliente do Minecraft. */
final class AuroraAssistantOverlay {
    private static final Color SURFACE = new Color(17, 11, 31, 246);
    private static final Color BORDER = new Color(196, 163, 255, 170);
    private static final Color TEXT = new Color(241, 235, 255);
    private static final Color MUTED = new Color(184, 173, 205);
    private static final AuroraAssistantOverlay INSTANCE = new AuroraAssistantOverlay();

    private AuroraIpcClient ipcClient;
    private JWindow window;
    private JTextArea conversation;
    private JTextField question;
    private JLabel status;
    private JLabel caption;
    private JButton sendButton;
    private JButton listenButton;
    private String pendingRequestId;
    private String voiceRequestId;

    private AuroraAssistantOverlay() { }

    static void toggle(AuroraIpcClient client) {
        INSTANCE.ipcClient = client;
        SwingUtilities.invokeLater(() -> {
            if (INSTANCE.window != null && INSTANCE.window.isVisible()) {
                INSTANCE.close();
            } else {
                INSTANCE.open();
            }
        });
    }

    static void receive(String message) {
        SwingUtilities.invokeLater(() -> INSTANCE.receiveOnUi(message));
    }

    private void open() {
        if (GraphicsEnvironment.isHeadless()) {
            System.err.println("[Aurora] O ambiente gráfico do Assistente está indisponível.");
            return;
        }
        if (window == null) buildWindow();
        Rectangle screen = GraphicsEnvironment.getLocalGraphicsEnvironment().getMaximumWindowBounds();
        int x = screen.x + screen.width - window.getWidth() - 28;
        int y = screen.y + screen.height - window.getHeight() - 28;
        window.setLocation(Math.max(screen.x, x), Math.max(screen.y, y));
        window.setVisible(true);
        window.toFront();
        question.requestFocusInWindow();
        System.out.println("[Aurora] Painel do Assistente aberto dentro do jogo.");
    }

    private void close() {
        if (window != null) {
            window.setVisible(false);
            window.dispose();
            window = null;
        }
    }

    private void buildWindow() {
        window = new JWindow();
        window.setAlwaysOnTop(true);
        window.setFocusableWindowState(true);
        window.setBackground(new Color(0, 0, 0, 0));
        window.setSize(new Dimension(520, 390));

        JPanel root = new JPanel(new BorderLayout(12, 10));
        root.setBackground(SURFACE);
        root.setBorder(BorderFactory.createCompoundBorder(
            BorderFactory.createLineBorder(BORDER, 1, true),
            BorderFactory.createEmptyBorder(14, 14, 12, 14)));

        JPanel header = new JPanel(new BorderLayout(10, 0));
        header.setOpaque(false);
        AuroraMark mark = new AuroraMark();
        header.add(mark, BorderLayout.WEST);
        JLabel title = new JLabel("<html><b>Aurora</b><br><span style='font-size:9px'>Assistente no jogo</span></html>");
        title.setForeground(TEXT);
        title.setFont(new Font(Font.SANS_SERIF, Font.PLAIN, 16));
        header.add(title, BorderLayout.CENTER);
        JButton close = button("×");
        close.addActionListener(event -> close());
        header.add(close, BorderLayout.EAST);
        root.add(header, BorderLayout.NORTH);

        conversation = new JTextArea("Pergunte sobre Minecraft, mods, desempenho ou um erro.\n");
        conversation.setEditable(false);
        conversation.setLineWrap(true);
        conversation.setWrapStyleWord(true);
        conversation.setBackground(new Color(9, 6, 18));
        conversation.setForeground(TEXT);
        conversation.setCaretColor(TEXT);
        conversation.setFont(new Font(Font.SANS_SERIF, Font.PLAIN, 13));
        conversation.setBorder(BorderFactory.createEmptyBorder(10, 10, 10, 10));
        JScrollPane scroll = new JScrollPane(conversation);
        scroll.setBorder(BorderFactory.createLineBorder(new Color(255, 255, 255, 24)));
        root.add(scroll, BorderLayout.CENTER);

        JPanel bottom = new JPanel(new BorderLayout(7, 6));
        bottom.setOpaque(false);
        caption = new JLabel(" ", SwingConstants.CENTER);
        caption.setForeground(new Color(231, 214, 255));
        caption.setFont(new Font(Font.SANS_SERIF, Font.BOLD, 12));
        bottom.add(caption, BorderLayout.NORTH);

        JPanel composer = new JPanel(new BorderLayout(6, 0));
        composer.setOpaque(false);
        question = new JTextField();
        question.setToolTipText("Pergunte ao Aurora");
        question.setBackground(new Color(255, 255, 255, 16));
        question.setForeground(TEXT);
        question.setCaretColor(TEXT);
        question.setBorder(BorderFactory.createCompoundBorder(
            BorderFactory.createLineBorder(new Color(255, 255, 255, 28)),
            BorderFactory.createEmptyBorder(7, 8, 7, 8)));
        question.addActionListener(event -> send(false));
        composer.add(question, BorderLayout.CENTER);
        JButton capture = button("Analisar tela");
        capture.setToolTipText("Pede confirmação antes de capturar");
        capture.addActionListener(event -> send(true));
        composer.add(capture, BorderLayout.WEST);
        listenButton = button("Falar");
        listenButton.setToolTipText("Fazer uma pergunta usando o microfone do launcher");
        listenButton.addActionListener(event -> listen());
        composer.add(listenButton, BorderLayout.NORTH);
        sendButton = button("Enviar");
        sendButton.addActionListener(event -> send(false));
        composer.add(sendButton, BorderLayout.EAST);
        bottom.add(composer, BorderLayout.CENTER);
        status = new JLabel("Pronto", SwingConstants.LEFT);
        status.setForeground(MUTED);
        status.setFont(new Font(Font.SANS_SERIF, Font.PLAIN, 10));
        bottom.add(status, BorderLayout.SOUTH);
        root.add(bottom, BorderLayout.SOUTH);
        window.setContentPane(root);

        bindCloseShortcut(root, KeyStroke.getKeyStroke(KeyEvent.VK_ESCAPE, 0));
        bindCloseShortcut(root, KeyStroke.getKeyStroke(KeyEvent.VK_SLASH, InputEvent.ALT_GRAPH_DOWN_MASK));
    }

    private void bindCloseShortcut(JComponent root, KeyStroke keyStroke) {
        String actionName = "aurora-close-" + keyStroke;
        root.getInputMap(JComponent.WHEN_IN_FOCUSED_WINDOW).put(keyStroke, actionName);
        root.getActionMap().put(actionName, new AbstractAction() {
            @Override public void actionPerformed(ActionEvent event) { close(); }
        });
    }

    private JButton button(String label) {
        JButton button = new JButton(label);
        button.setFocusable(false);
        button.setForeground(TEXT);
        button.setBackground(new Color(119, 86, 181));
        button.setBorder(BorderFactory.createEmptyBorder(7, 10, 7, 10));
        return button;
    }

    private void send(boolean withScreenshot) {
        String text = question.getText().trim();
        if (text.isEmpty() || pendingRequestId != null || ipcClient == null || !ipcClient.isOpen()) return;
        String screenshot = null;
        if (withScreenshot) {
            int choice = JOptionPane.showConfirmDialog(
                window,
                "O Aurora capturará a tela atual e a enviará ao Gemini somente para responder esta pergunta. Continuar?",
                "Autorizar análise da tela",
                JOptionPane.YES_NO_OPTION,
                JOptionPane.WARNING_MESSAGE);
            if (choice != JOptionPane.YES_OPTION) return;
            try {
                screenshot = captureScreen();
            } catch (Exception error) {
                status.setText("Não foi possível capturar a tela.");
                return;
            }
        }
        pendingRequestId = UUID.randomUUID().toString();
        conversation.append("\nVocê: " + text + "\n");
        question.setText("");
        question.setEnabled(false);
        sendButton.setEnabled(false);
        status.setText("Aurora está pensando…");
        ipcClient.publishAssistantRequest(pendingRequestId, text, screenshot);
    }

    private void listen() {
        if (voiceRequestId != null || pendingRequestId != null || ipcClient == null || !ipcClient.isOpen()) return;
        voiceRequestId = UUID.randomUUID().toString();
        listenButton.setEnabled(false);
        status.setText("Ouvindo você…");
        ipcClient.publishAssistantListen(voiceRequestId);
    }

    private String captureScreen() throws Exception {
        Rectangle bounds = GraphicsEnvironment.getLocalGraphicsEnvironment().getMaximumWindowBounds();
        BufferedImage source = new Robot().createScreenCapture(bounds);
        int width = Math.min(1280, source.getWidth());
        int height = Math.max(1, source.getHeight() * width / source.getWidth());
        Image scaled = source.getScaledInstance(width, height, Image.SCALE_SMOOTH);
        BufferedImage output = new BufferedImage(width, height, BufferedImage.TYPE_INT_RGB);
        Graphics2D graphics = output.createGraphics();
        graphics.drawImage(scaled, 0, 0, null);
        graphics.dispose();
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        ImageIO.write(output, "jpg", bytes);
        return "data:image/jpeg;base64," + Base64.getEncoder().encodeToString(bytes.toByteArray());
    }

    private void receiveOnUi(String message) {
        String kind = jsonString(message, "kind");
        String requestId = jsonString(message, "requestId");
        if ("assistantTranscript".equals(kind) && requestId.equals(voiceRequestId)) {
            String text = jsonString(message, "text");
            String error = jsonString(message, "error");
            voiceRequestId = null;
            listenButton.setEnabled(true);
            if (!error.isEmpty() || text.isEmpty()) {
                status.setText(error.isEmpty() ? "Nenhuma fala foi detectada." : error);
                return;
            }
            question.setText(text);
            status.setText("Pergunta transcrita");
            send(false);
            return;
        }
        if ("assistantCaption".equals(kind) && requestId.equals(pendingRequestId)) {
            caption.setText(jsonString(message, "caption"));
            return;
        }
        if (!"assistantResponse".equals(kind) || !requestId.equals(pendingRequestId)) return;
        String text = jsonString(message, "text");
        String error = jsonString(message, "error");
        conversation.append("\nAurora: " + (error.isEmpty() ? text : error) + "\n");
        conversation.setCaretPosition(conversation.getDocument().getLength());
        status.setText(error.isEmpty() ? "Resposta pronta" : "Não foi possível responder");
        question.setEnabled(true);
        sendButton.setEnabled(true);
        question.requestFocusInWindow();
        pendingRequestId = null;
    }

    private static String jsonString(String json, String name) {
        String marker = "\"" + name + "\":";
        int start = json.indexOf(marker);
        if (start < 0) return "";
        start += marker.length();
        while (start < json.length() && Character.isWhitespace(json.charAt(start))) start++;
        if (start >= json.length() || json.charAt(start) != '"') return "";
        StringBuilder value = new StringBuilder();
        boolean escaped = false;
        for (int index = start + 1; index < json.length(); index++) {
            char current = json.charAt(index);
            if (escaped) {
                if (current == 'n') value.append('\n');
                else if (current == 'r') value.append('\r');
                else if (current == 't') value.append('\t');
                else value.append(current);
                escaped = false;
            } else if (current == '\\') {
                escaped = true;
            } else if (current == '"') {
                return value.toString();
            } else {
                value.append(current);
            }
        }
        return "";
    }

    private static final class AuroraMark extends JComponent {
        private float phase;

        AuroraMark() {
            setPreferredSize(new Dimension(44, 44));
            Timer timer = new Timer(45, event -> { phase += 0.12F; repaint(); });
            timer.start();
        }

        @Override protected void paintComponent(Graphics base) {
            Graphics2D graphics = (Graphics2D) base.create();
            graphics.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON);
            float pulse = (float) ((Math.sin(phase) + 1.0) * 0.5);
            int alpha = 75 + Math.round(pulse * 80);
            graphics.setColor(new Color(171, 123, 255, alpha));
            graphics.fillOval(3, 3, 38, 38);
            graphics.setColor(new Color(239, 220, 255));
            graphics.setStroke(new BasicStroke(2.2F));
            graphics.drawLine(22, 9, 22, 35);
            graphics.drawLine(10, 22, 34, 22);
            graphics.drawLine(14, 14, 30, 30);
            graphics.drawLine(30, 14, 14, 30);
            graphics.dispose();
        }
    }
}
