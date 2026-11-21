import sys
import os
import time
import random
import threading
import select
import termios
import tty

class RacingGame:
    def __init__(self, width=80, height=24):
        self.width = width
        self.height = height
        self.player_x = width // 2
        self.player_y = height - 2
        self.player_dx = 0
        self.enemies = []
        self.score = 0
        self.lives = 3
        self.level = 1
        self.spawn_rate = 50
        self.spawn_counter = 0
        self.running = True
        self.game_over = False
        self.best_score = 0
        self.old_settings = None

    def _is_tty(self):
        return sys.stdin.isatty()

    def _setup_terminal(self):
        if self._is_tty():
            self.old_settings = termios.tcgetattr(sys.stdin)
            tty.setraw(sys.stdin)
            tty.setcbreak(sys.stdin)

    def _restore_terminal(self):
        if self.old_settings is not None:
            termios.tcsetattr(sys.stdin, termios.TCSADRAIN, self.old_settings)

    def draw(self):
        os.system('cls' if os.name == 'nt' else 'clear')
        for y in range(self.height):
            row = ''
            for x in range(self.width):
                if x == 0 or x == self.width - 1 or y == 0 or y == self.height - 1:
                    row += '│'
                elif y == self.player_y and x == self.player_x:
                    row += '█'
                else:
                    row += ' '
            print(row)
        print(f"Score: {self.score} | Lives: {self.lives} | Level: {self.level}")

    def spawn_enemy(self):
        x = random.randint(1, self.width - 2)
        self.enemies.append({'x': x, 'y': 0, 'speed': random.randint(1, 2)})

    def move_player(self, dx):
        self.player_dx = dx
        self.player_x += dx
        if self.player_x < 1:
            self.player_x = 1
        elif self.player_x > self.width - 2:
            self.player_x = self.width - 2

    def update(self):
        self.update_enemies()
        self.spawn_enemy()
        self.check_collisions()
        self.next_level()
        self.draw()

    def update_enemies(self):
        for enemy in self.enemies:
            enemy['y'] += enemy['speed']
            if enemy['y'] >= self.height:
                self.enemies.remove(enemy)
            else:
                if self.check_collision(self.player_x, self.player_y, enemy['x'], enemy['y']):
                    self.lives -= 1
                    self.enemies.remove(enemy)
                    if self.lives <= 0:
                        self.game_over = True

    def check_collision(self, ax, ay, bx, by):
        return ax < bx + 1 and ax + 1 > bx and ay < by + 1 and ay + 1 > by

    def check_collisions(self):
        for enemy in self.enemies:
            if self.check_collision(self.player_x, self.player_y, enemy['x'], enemy['y']):
                self.lives -= 1
                self.enemies.remove(enemy)
                if self.lives <= 0:
                    self.game_over = True

    def next_level(self):
        if self.score >= 1000 * self.level:
            self.level += 1
            self.spawn_rate = max(10, self.spawn_rate - 5)
            self.score = 0

    def game_over(self):
        print(f"Game Over! Score: {self.score} | Best: {self.best_score} | Lives: {self.lives}")
        if self.score > self.best_score:
            self.best_score = self.score
        choice = input("Press 'r' to restart or 'q' to quit: ").lower()
        if choice == 'r':
            self.__init__(self.width, self.height)
            self.running = True
            self.game_over = False
        elif choice == 'q':
            self.running = False

    def _input_thread(self):
        while self.running:
            if self._is_tty():
                r, _, _ = select.select([sys.stdin], [], [], 0.01)
                if r:
                    key = sys.stdin.read(1)
                    if key == 'a' or key == '\x1b[D':
                        self.player_dx = -1
                    elif key == 'd' or key == '\x1b[C':
                        self.player_dx = 1
                    elif key == 'q':
                        self.running = False
                    elif key == 'r':
                        self.game_over = False
                        self.__init__(self.width, self.height)
                        self.running = True

def main():
    game = RacingGame()
    game._setup_terminal()
    try:
        while game.running:
            if game.game_over:
                game.game_over()
                continue
            game.move_player(game.player_dx)
            game.update()
            time.sleep(0.016)
    finally:
        game._restore_terminal()

if __name__ == "__main__":
    main()