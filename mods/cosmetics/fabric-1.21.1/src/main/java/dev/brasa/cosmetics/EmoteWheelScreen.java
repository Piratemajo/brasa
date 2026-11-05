package dev.brasa.cosmetics;

import java.util.List;
import net.minecraft.client.gui.DrawContext;
import net.minecraft.client.gui.screen.Screen;
import net.minecraft.text.Text;

/** Rueda de emotes: se abre con B, se elige con el ratón y se suelta B (o se hace clic). */
final class EmoteWheelScreen extends Screen {
    private static final int RADIUS = 86;
    private static final int W = 96;
    private static final int H = 22;
    private int hovered = -1;

    EmoteWheelScreen() {
        super(Text.translatable("screen.brasa-cosmetics.wheel"));
    }

    private List<Loadout.EmoteOut> emotes() {
        return State.loadout == null ? List.of() : State.loadout.emotes;
    }

    @Override
    public boolean shouldPause() {
        return false;
    }

    @Override
    public void render(DrawContext ctx, int mouseX, int mouseY, float delta) {
        super.render(ctx, mouseX, mouseY, delta);
        List<Loadout.EmoteOut> list = emotes();
        int cx = width / 2;
        int cy = height / 2;
        if (list.isEmpty()) {
            ctx.drawCenteredTextWithShadow(textRenderer, Text.translatable("screen.brasa-cosmetics.empty"), cx, cy, 0xFFE8DCCB);
            return;
        }
        // el elegido es el más cercano en ángulo al ratón (si se aleja del centro)
        double dx = mouseX - cx;
        double dy = mouseY - cy;
        hovered = -1;
        if (dx * dx + dy * dy > 18 * 18) {
            double ang = Math.toDegrees(Math.atan2(dy, dx)) + 90;
            ang = ((ang % 360) + 360) % 360;
            hovered = (int) Math.round(ang / (360.0 / list.size())) % list.size();
        }
        for (int k = 0; k < list.size(); k++) {
            double a = Math.toRadians(-90 + 360.0 * k / list.size());
            int x = cx + (int) Math.round(Math.cos(a) * RADIUS) - W / 2;
            int y = cy + (int) Math.round(Math.sin(a) * RADIUS) - H / 2;
            boolean on = k == hovered;
            ctx.fill(x, y, x + W, y + H, on ? 0xE6FF8A2B : 0xC81E1A17);
            ctx.drawBorder(x, y, W, H, on ? 0xFFFFB85C : 0xFF433B35);
            ctx.drawCenteredTextWithShadow(textRenderer, Text.literal(list.get(k).name), x + W / 2, y + 7, on ? 0xFF221006 : 0xFFEFE7DB);
        }
        ctx.drawCenteredTextWithShadow(textRenderer, title, cx, cy - 4, 0xFFFFB85C);
    }

    private void choose() {
        List<Loadout.EmoteOut> list = emotes();
        if (hovered >= 0 && hovered < list.size()) {
            Emotes.play(list.get(hovered));
        }
        close();
    }

    @Override
    public boolean mouseClicked(double mouseX, double mouseY, int button) {
        choose();
        return true;
    }

    @Override
    public boolean keyReleased(int keyCode, int scanCode, int modifiers) {
        if (BrasaCosmetics.WHEEL.matchesKey(keyCode, scanCode)) {
            choose();
            return true;
        }
        return super.keyReleased(keyCode, scanCode, modifiers);
    }
}
