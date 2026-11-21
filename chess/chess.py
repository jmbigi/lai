import sys
import re

class Chess:
    def __init__(self):
        # Inicializar tablero vacío
        self.board = [[' ' for _ in range(8)] for _ in range(8)]
        # Row 0 (8th rank) - Black pieces (top of board)
        self.board[0] = ['\u265c', '\u265e', '\u265d', '\u265b', '\u265a', '\u265d', '\u265e', '\u265c']
        # Row 1 (7th rank) - Black pawns
        self.board[1] = ['\u265f'] * 8
        # Row 6 (2nd rank) - White pawns
        self.board[6] = ['\u2659'] * 8
        # Row 7 (1st rank) - White pieces (bottom of board)
        self.board[7] = ['\u2656', '\u2658', '\u2657', '\u2655', '\u2654', '\u2657', '\u2658', '\u2656']
        self.turn = 'white'
        self.move_count = 0
        self.enpassant = None
        self.castle_rights = {'white': {'kingside': True, 'queenside': True}, 'black': {'kingside': True, 'queenside': True}}
        self.game_over = False
        self.history = []

    def is_white_piece(self, piece):
        return piece in ('\u2654', '\u2655', '\u2656', '\u2657', '\u2658', '\u2659')

    def is_black_piece(self, piece):
        return piece in ('\u265a', '\u265b', '\u265c', '\u265d', '\u265e', '\u265f')

    def print_board(self):
        print('  a b c d e f g h')
        for i in range(8):
            print(f"{8 - i} {' '.join(self.board[i])} {8 - i}")
        print('  a b c d e f g h')

    def get_piece(self, pos):
        x, y = pos
        return self.board[x][y]

    def is_valid_pos(self, pos):
        x, y = pos
        return 0 <= x < 8 and 0 <= y < 8

    def move_piece(self, start, end):
        x1, y1 = start
        x2, y2 = end
        piece = self.get_piece(start)
        if not piece:
            return False
        if self.turn == 'white' and self.is_black_piece(piece):
            return False
        if self.turn == 'black' and self.is_white_piece(piece):
            return False
        if self.is_valid_move(start, end):
            self.board[x2][y2] = self.board[x1][y1]
            self.board[x1][y1] = ' '
            self.move_count += 1
            self.history.append(f"{self.algebraic_notation(start)}{self.algebraic_notation(end)}")
            self.turn = 'black' if self.turn == 'white' else 'white'
            self.check_game_state()
            return True
        return False

    def is_valid_move(self, start, end):
        piece = self.get_piece(start)
        x1, y1 = start
        x2, y2 = end
        if self.get_piece(end) == piece:
            return False
        if self.is_check(start, end):
            return False
        if self.is_check_after_move(start, end):
            return False
        if piece == '\u2659':
            return self.is_valid_pawn_move(start, end, False)
        elif piece == '\u265f':
            return self.is_valid_pawn_move(start, end, True)
        elif piece == '\u2656':
            return self.is_valid_rook_move(start, end, False)
        elif piece == '\u265c':
            return self.is_valid_rook_move(start, end, True)
        elif piece == '\u2658':
            return self.is_valid_knight_move(start, end, False)
        elif piece == '\u265e':
            return self.is_valid_knight_move(start, end, True)
        elif piece == '\u2657':
            return self.is_valid_bishop_move(start, end, False)
        elif piece == '\u265d':
            return self.is_valid_bishop_move(start, end, True)
        elif piece == '\u2655':
            return self.is_valid_queen_move(start, end, False)
        elif piece == '\u265b':
            return self.is_valid_queen_move(start, end, True)
        elif piece == '\u2654':
            return self.is_valid_king_move(start, end, False)
        elif piece == '\u265a':
            return self.is_valid_king_move(start, end, True)
        return False

    def is_valid_pawn_move(self, start, end, is_black):
        x1, y1 = start
        x2, y2 = end
        piece = self.get_piece(start)
        direction = -1 if not is_black else 1
        if y2 == y1:
            if x2 == x1 + direction:
                if self.get_piece(end) == ' ':
                    return True
            elif x2 == x1 + 2 * direction:
                if x1 in (1, 6) and self.get_piece((x1 + direction, y1)) == ' ' and self.get_piece(end) == ' ':
                    return True
        else:
            if abs(y2 - y1) == 1 and x2 == x1 + direction:
                if self.get_piece(end) != ' ':
                    return True
                if self.enpassant == end:
                    return True
        return False

    def is_valid_rook_move(self, start, end, is_black):
        x1, y1 = start
        x2, y2 = end
        if x1 == x2:
            for y in range(min(y1, y2) + 1, max(y1, y2)):
                if self.get_piece((x1, y)) != ' ':
                    return False
        elif y1 == y2:
            for x in range(min(x1, x2) + 1, max(x1, x2)):
                if self.get_piece((x, y1)) != ' ':
                    return False
        else:
            return False
        return True

    def is_valid_knight_move(self, start, end, is_black):
        x1, y1 = start
        x2, y2 = end
        dx = abs(x2 - x1)
        dy = abs(y2 - y1)
        return (dx == 2 and dy == 1) or (dx == 1 and dy == 2)

    def is_valid_bishop_move(self, start, end, is_black):
        x1, y1 = start
        x2, y2 = end
        if abs(x2 - x1) == abs(y2 - y1):
            dx = x2 - x1
            dy = y2 - y1
            step = 1 if dx > 0 else -1
            for i in range(1, abs(dx)):
                x = x1 + i * step
                y = y1 + i * dy // dx
                if self.get_piece((x, y)) != ' ':
                    return False
        else:
            return False
        return True

    def is_valid_queen_move(self, start, end, is_black):
        return self.is_valid_rook_move(start, end, is_black) or self.is_valid_bishop_move(start, end, is_black)

    def is_valid_king_move(self, start, end, is_black):
        x1, y1 = start
        x2, y2 = end
        dx = abs(x2 - x1)
        dy = abs(y2 - y1)
        return dx <= 1 and dy <= 1

    def is_check(self, start, end):
        x1, y1 = start
        x2, y2 = end
        piece = self.get_piece(start)
        if self.turn == 'white':
            enemy = 'black'
        else:
            enemy = 'white'
        if piece == '\u2654' or piece == '\u265a':
            return self.is_in_check(x2, y2, enemy)
        return False

    def is_in_check(self, x, y, color):
        for i in range(8):
            for j in range(8):
                piece = self.get_piece((i, j))
                if (self.is_white_piece(piece) and color == 'black') or (self.is_black_piece(piece) and color == 'white'):
                    if self.is_attacked((i, j), (x, y), color):
                        return True
        return False

    def is_attacked(self, start, end, color):
        x1, y1 = start
        x2, y2 = end
        piece = self.get_piece(start)
        if piece == '\u2659' or piece == '\u265f':
            return self.is_pawn_attacking(start, end, color)
        elif piece == '\u2656' or piece == '\u265c':
            return self.is_rook_attacking(start, end, color)
        elif piece == '\u2658' or piece == '\u265e':
            return self.is_knight_attacking(start, end, color)
        elif piece == '\u2657' or piece == '\u265d':
            return self.is_bishop_attacking(start, end, color)
        elif piece == '\u2655' or piece == '\u265b':
            return self.is_queen_attacking(start, end, color)
        elif piece == '\u2654' or piece == '\u265a':
            return self.is_king_attacking(start, end, color)
        return False

    def is_pawn_attacking(self, start, end, color):
        x1, y1 = start
        x2, y2 = end
        if color == 'white':
            if x2 == x1 - 1 and abs(y2 - y1) == 1 and self.get_piece(end) != ' ':
                return True
        else:
            if x2 == x1 + 1 and abs(y2 - y1) == 1 and self.get_piece(end) != ' ':
                return True
        return False

    def is_rook_attacking(self, start, end, color):
        x1, y1 = start
        x2, y2 = end
        if x1 == x2:
            for y in range(min(y1, y2) + 1, max(y1, y2)):
                if self.get_piece((x1, y)) != ' ':
                    return False
            return True
        elif y1 == y2:
            for x in range(min(x1, x2) + 1, max(x1, x2)):
                if self.get_piece((x, y1)) != ' ':
                    return False
            return True
        return False

    def is_knight_attacking(self, start, end, color):
        x1, y1 = start
        x2, y2 = end
        dx = abs(x2 - x1)
        dy = abs(y2 - y1)
        return (dx == 2 and dy == 1) or (dx == 1 and dy == 2)

    def is_bishop_attacking(self, start, end, color):
        x1, y1 = start
        x2, y2 = end
        if abs(x2 - x1) == abs(y2 - y1):
            dx = x2 - x1
            dy = y2 - y1
            step = 1 if dx > 0 else -1
            for i in range(1, abs(dx)):
                x = x1 + i * step
                y = y1 + i * dy // dx
                if self.get_piece((x, y)) != ' ':
                    return False
            return True
        return False

    def is_queen_attacking(self, start, end, color):
        return self.is_rook_attacking(start, end, color) or self.is_bishop_attacking(start, end, color)

    def is_king_attacking(self, start, end, color):
        x1, y1 = start
        x2, y2 = end
        dx = abs(x2 - x1)
        dy = abs(y2 - y1)
        return dx <= 1 and dy <= 1

    def is_check_after_move(self, start, end):
        x1, y1 = start
        x2, y2 = end
        piece = self.get_piece(start)
        if self.turn == 'white':
            enemy = 'black'
        else:
            enemy = 'white'
        if piece == '\u2654' or piece == '\u265a':
            return self.is_in_check(x2, y2, enemy)
        return False

    def check_game_state(self):
        king_pos = None
        king_piece = '\u2654' if self.turn == 'white' else '\u265a'
        for i in range(8):
            for j in range(8):
                if self.get_piece((i, j)) == king_piece:
                    king_pos = (i, j)
                    break
            if king_pos:
                break
        
        if king_pos and self.is_in_check(king_pos[0], king_pos[1], self.turn):
            if self.is_checkmate():
                print("Jaque mate!")
                self.game_over = True
            else:
                print("Jaque!")
        elif self.is_stalemate():
            print("Tablas!")
            self.game_over = True
        elif self.move_count >= 144:
            print("Tablas por repetición de movimientos!")
            self.game_over = True

    def is_checkmate(self):
        for i in range(8):
            for j in range(8):
                piece = self.get_piece((i, j))
                if (self.is_white_piece(piece) and self.turn == 'white') or (self.is_black_piece(piece) and self.turn == 'black'):
                    for x in range(8):
                        for y in range(8):
                            if self.is_valid_move((i, j), (x, y)):
                                return False
        return True

    def is_stalemate(self):
        for i in range(8):
            for j in range(8):
                piece = self.get_piece((i, j))
                if (self.is_white_piece(piece) and self.turn == 'white') or (self.is_black_piece(piece) and self.turn == 'black'):
                    for x in range(8):
                        for y in range(8):
                            if self.is_valid_move((i, j), (x, y)):
                                return False
        return True

    def algebraic_notation(self, pos):
        x, y = pos
        return chr(ord('a') + y) + str(8 - x)

    def run(self):
        while not self.game_over:
            self.print_board()
            print(f"Turno: {self.turn}")
            move = input("Ingresa movimiento (ej: e2 e4) o 'quit' para salir: ").strip()
            if move == 'quit':
                print("Juego terminado.")
                break
            if not move:
                continue
            match = re.match(r'^([a-h][1-8])\s+([a-h][1-8])$', move)
            if not match:
                print("Movimiento inválido.")
                continue
            start = match.group(1)
            end = match.group(2)
            start = (8 - int(start[1]), ord(start[0]) - ord('a'))
            end = (8 - int(end[1]), ord(end[0]) - ord('a'))
            if self.move_piece(start, end):
                self.check_game_state()
            else:
                print("Movimiento inválido.")

if __name__ == "__main__":
    game = Chess()
    game.run()
