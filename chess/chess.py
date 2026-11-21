import sys
import re
import time
from collections import deque

class Chess:
    def __init__(self):
        self.board = [[' ' for _ in range(8)] for _ in range(8)]
        self.setup()
        self.turn = 'white'
        self.move_count = 0
        self.history = []
        self.enpassant = None
        self.castle_rights = {'white': {'kingside': True, 'queenside': True}, 'black': {'kingside': True, 'queenside': True}}
        self.promotion = None
        self.game_over = False
        self.move_notation = []

    def setup(self):
        for i in range(8):
            self.board[0][i] = '♖' if i == 0 or i == 7 else '♘' if i == 1 or i == 6 else '♗' if i == 2 or i == 5 else '♕' if i == 3 else '♔'
            self.board[7][i] = '♜' if i == 0 or i == 7 else '♞' if i == 1 or i == 6 else '♝' if i == 2 or i == 5 else '♛' if i == 3 else '♚'

        for i in range(8):
            self.board[1][i] = '♙' if i == 0 or i == 7 else '♘' if i == 1 or i == 6 else '♗' if i == 2 or i == 5 else '♕' if i == 3 else '♔'
            self.board[6][i] = '♟' if i == 0 or i == 7 else '♞' if i == 1 or i == 6 else '♝' if i == 2 or i == 5 else '♛' if i == 3 else '♚'

    def print_board(self):
        print("  a b c d e f g h")
        for i in range(8):
            print(f"{i+1} {' '.join(self.board[i])}")
        print()

    def get_piece(self, pos):
        x, y = pos
        return self.board[x][y]

    def is_valid_pos(self, pos):
        x, y = pos
        return 0 <= x < 8 and 0 <= y < 8

    def is_empty(self, pos):
        return self.get_piece(pos) == ' '

    def is_enemy(self, pos, color):
        piece = self.get_piece(pos)
        return piece.islower() if color == 'white' else piece.isupper()

    def is_friend(self, pos, color):
        piece = self.get_piece(pos)
        return piece.isupper() if color == 'white' else piece.islower()

    def is_check(self, color):
        king_pos = self.find_king(color)
        for x in range(8):
            for y in range(8):
                piece = self.get_piece((x, y))
                if self.is_friend((x, y), color) and piece in 'Kk':
                    if self.is_attacked((x, y), color):
                        return True
        return False

    def find_king(self, color):
        for x in range(8):
            for y in range(8):
                piece = self.get_piece((x, y))
                if (piece == 'K' if color == 'white' else piece == 'k'):
                    return (x, y)
        return None

    def is_attacked(self, pos, color):
        x, y = pos
        for dx in [-1, 0, 1]:
            for dy in [-1, 0, 1]:
                if dx == 0 and dy == 0:
                    continue
                nx, ny = x + dx, y + dy
                if 0 <= nx < 8 and 0 <= ny < 8:
                    piece = self.get_piece((nx, ny))
                    if self.is_enemy((nx, ny), color):
                        if self.is_valid_move((nx, ny), (x, y), color):
                            return True
        return False

    def is_valid_move(self, start, end, color):
        piece = self.get_piece(start)
        if self.is_empty(start) or self.is_friend(start, color):
            return False
        if piece == 'P' or piece == 'p':
            return self.is_valid_pawn_move(start, end, color)
        elif piece == 'R' or piece == 'r':
            return self.is_valid_rook_move(start, end)
        elif piece == 'N' or piece == 'n':
            return self.is_valid_knight_move(start, end)
        elif piece == 'B' or piece == 'b':
            return self.is_valid_bishop_move(start, end)
        elif piece == 'Q' or piece == 'q':
            return self.is_valid_queen_move(start, end)
        elif piece == 'K' or piece == 'k':
            return self.is_valid_king_move(start, end)
        return False

    def is_valid_pawn_move(self, start, end, color):
        x, y = start
        ex, ey = end
        piece = self.get_piece(start)
        if color == 'white':
            direction = -1
            start_rank = 1
            end_rank = 0
        else:
            direction = 1
            start_rank = 6
            end_rank = 7

        if piece == 'P':
            if self.is_empty(end):
                if ex == x and ey == y + direction:
                    if y == start_rank and ey == y + 2 * direction:
                        return self.is_empty((x, y + direction)) and self.is_empty((x, y + direction * 2))
                    return True
                elif ex == x and ey == y + direction:
                    return True
                elif ex == x + 1 and ey == y + direction:
                    return self.is_enemy(end, color)
                elif ex == x - 1 and ey == y + direction:
                    return self.is_enemy(end, color)
                elif ex == x and ey == y + direction * 2:
                    return self.is_empty((x, y + direction)) and self.is_empty((x, y + direction * 2))
            elif self.is_enemy(end, color):
                if ex == x + 1 or ex == x - 1 and ey == y + direction:
                    return True
            elif ex == x and ey == y + direction:
                return True
            elif ex == x and ey == y + direction * 2:
                return self.is_empty((x, y + direction)) and self.is_empty((x, y + direction * 2))
        elif piece == 'p':
            if self.is_empty(end):
                if ex == x and ey == y + direction:
                    if y == start_rank and ey == y + 2 * direction:
                        return self.is_empty((x, y + direction)) and self.is_empty((x, y + direction * 2))
                    return True
                elif ex == x and ey == y + direction:
                    return True
                elif ex == x + 1 and ey == y + direction:
                    return self.is_enemy(end, color)
                elif ex == x - 1 and ey == y + direction:
                    return self.is_enemy(end, color)
                elif ex == x and ey == y + direction * 2:
                    return self.is_empty((x, y + direction)) and self.is_empty((x, y + direction * 2))
            elif self.is_enemy(end, color):
                if ex == x + 1 or ex == x - 1 and ey == y + direction:
                    return True
            elif ex == x and ey == y + direction:
                return True
            elif ex == x and ey == y + direction * 2:
                return self.is_empty((x, y + direction)) and self.is_empty((x, y + direction * 2))
        return False

    def is_valid_rook_move(self, start, end):
        x, y = start
        ex, ey = end
        if x == ex:
            for i in range(min(y, ey) + 1, max(y, ey)):
                if not self.is_empty((x, i)):
                    return False
        elif y == ey:
            for i in range(min(x, ex) + 1, max(x, ex)):
                if not self.is_empty((i, y)):
                    return False
        return True

    def is_valid_knight_move(self, start, end):
        x, y = start
        ex, ey = end
        dx = abs(ex - x)
        dy = abs(ey - y)
        return (dx == 2 and dy == 1) or (dx == 1 and dy == 2)

    def is_valid_bishop_move(self, start, end):
        x, y = start
        ex, ey = end
        if abs(ex - x) != abs(ey - y):
            return False
        dx = ex - x
        dy = ey - y
        step = 1 if dx > 0 else -1
        for i in range(1, abs(dx)):
            nx = x + i * step
            ny = y + i * dy // dx
            if not self.is_empty((nx, ny)):
                return False
        return True

    def is_valid_queen_move(self, start, end):
        return self.is_valid_rook_move(start, end) or self.is_valid_bishop_move(start, end)

    def is_valid_king_move(self, start, end):
        x, y = start
        ex, ey = end
        dx = abs(ex - x)
        dy = abs(ey - y)
        return dx <= 1 and dy <= 1

    def move(self, start, end):
        if self.game_over:
            return False
        if not self.is_valid_pos(start) or not self.is_valid_pos(end):
            return False
        piece = self.get_piece(start)
        if self.is_empty(start) or self.is_friend(start, self.turn):
            return False
        if not self.is_valid_move(start, end, self.turn):
            return False
        if self.is_check(self.turn):
            return False
        self.move_count += 1
        self.history.append((start, end, piece))
        self.move_notation.append(self.get_notation(start, end, piece))
        self.board[end[0]][end[1]] = piece
        self.board[start[0]][start[1]] = ' '
        self.turn = 'black' if self.turn == 'white' else 'white'
        self.check_game_over()
        return True

    def get_notation(self, start, end, piece):
        x, y = start
        ex, ey = end
        piece_type = piece.lower()
        if piece_type == 'p':
            return f"{chr(ord('a') + y)}{ex + 1}"
        elif piece_type == 'r':
            return f"{chr(ord('a') + y)}{ex + 1}"
        elif piece_type == 'n':
            return f"{chr(ord('a') + y)}{ex + 1}"
        elif piece_type == 'b':
            return f"{chr(ord('a') + y)}{ex + 1}"
        elif piece_type == 'q':
            return f"{chr(ord('a') + y)}{ex + 1}"
        elif piece_type == 'k':
            return f"{chr(ord('a') + y)}{ex + 1}"
        return ""

    def check_game_over(self):
        if self.is_check(self.turn):
            if self.is_check(self.turn):
                self.game_over = True
                print("Checkmate!")
                return
        if not self.has_moves(self.turn):
            self.game_over = True
            print("Stalemate!")
            return
        if self.move_count >= 100:
            self.game_over = True
            print("Draw by repetition!")
            return

    def has_moves(self, color):
        for x in range(8):
            for y in range(8):
                if self.is_friend((x, y), color):
                    piece = self.get_piece((x, y))
                    if piece in 'PNBRQK':
                        for dx in [-1, 0, 1]:
                            for dy in [-1, 0, 1]:
                                if dx == 0 and dy == 0:
                                    continue
                                nx, ny = x + dx, y + dy
                                if 0 <= nx < 8 and 0 <= ny < 8:
                                    if self.is_valid_move((x, y), (nx, ny), color):
                                        return True
        return False

    def play(self):
        while not self.game_over:
            self.print_board()
            move = input(f"{self.turn}'s move: ").strip()
            if move == 'quit':
                print("Game ended.")
                return
            if move == 'help':
                print("Commands: move [start] [end], quit, help")
                continue
            if len(move) < 4:
                print("Invalid move.")
                continue
            start = move[:2]
            end = move[2:]
            if not re.match(r'[a-h][1-8][a-h][1-8]', move):
                print("Invalid move.")
                continue
            start_col = ord(start[0]) - ord('a')
            start_row = int(start[1]) - 1
            end_col = ord(end[0]) - ord('a')
            end_row = int(end[1]) - 1
            start_pos = (start_row, start_col)
            end_pos = (end_row, end_col)
            if self.move(start_pos, end_pos):
                self.print_board()
            else:
                print("Invalid move.")
        print("Game over.")

if __name__ == "__main__":
    game = Chess()
    game.play()